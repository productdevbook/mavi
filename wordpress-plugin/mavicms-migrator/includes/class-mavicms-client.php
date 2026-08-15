<?php
/**
 * Talks to a Mavi CMS instance.
 *
 * @package MaviCMS_Migrator
 */

defined( 'ABSPATH' ) || exit;

/**
 * Thin HTTP client over the Mavi CMS REST API.
 *
 * Authentication is a session cookie: the password is exchanged for one at
 * connect time and never stored, so a database leak on the WordPress side does
 * not hand over the destination site.
 */
class MaviCMS_Client {

	const OPTION_SITE    = 'mavicms_site_url';
	const OPTION_SESSION = 'mavicms_session';
	const OPTION_PREFIX  = 'mavicms_api_prefix';

	/**
	 * Base URL of the Mavi CMS instance, without a trailing slash.
	 *
	 * @var string
	 */
	private $site_url;

	/**
	 * Session cookie value, or an empty string when not signed in.
	 *
	 * @var string
	 */
	private $session;

	/**
	 * Path the API is served under, discovered at connect time.
	 *
	 * A normal install puts the API behind `/api` on the site's own domain,
	 * but the backend can also be exposed directly, where there is no prefix.
	 *
	 * @var string
	 */
	private $prefix;

	/**
	 * Builds a client, falling back to the stored connection.
	 *
	 * @param string $site_url Base URL. Falls back to the stored one.
	 * @param string $session  Session cookie. Falls back to the stored one.
	 */
	public function __construct( $site_url = '', $session = '' ) {
		$this->site_url = untrailingslashit( $site_url ? $site_url : (string) get_option( self::OPTION_SITE, '' ) );
		$this->session  = $session ? $session : (string) get_option( self::OPTION_SESSION, '' );
		$this->prefix   = (string) get_option( self::OPTION_PREFIX, '/api' );
	}

	/**
	 * Where this client points.
	 *
	 * @return string Configured site URL.
	 */
	public function site_url() {
		return $this->site_url;
	}

	/**
	 * Whether a previous sign-in is still on record.
	 *
	 * @return bool Whether a session is stored.
	 */
	public function has_session() {
		return '' !== $this->session;
	}

	/**
	 * Signs in and stores the resulting session cookie.
	 *
	 * @param string $username Mavi CMS username.
	 * @param string $password Mavi CMS password.
	 * @return true|WP_Error
	 */
	public function connect( $username, $password ) {
		$prefix = $this->detect_prefix();
		if ( is_wp_error( $prefix ) ) {
			return $prefix;
		}
		$this->prefix = $prefix;

		$response = wp_remote_post(
			$this->endpoint( '/login' ),
			array(
				'timeout' => 30,
				'headers' => array( 'Content-Type' => 'application/json' ),
				'body'    => wp_json_encode(
					array(
						'username' => $username,
						'password' => $password,
					)
				),
			)
		);

		if ( is_wp_error( $response ) ) {
			return $response;
		}

		$code = wp_remote_retrieve_response_code( $response );
		if ( 200 !== $code ) {
			return new WP_Error( 'mavicms_login_failed', $this->error_message( $response, __( 'Sign-in failed.', 'mavicms-migrator' ) ) );
		}

		foreach ( wp_remote_retrieve_cookies( $response ) as $cookie ) {
			if ( 'mavicms_session' === $cookie->name ) {
				$this->session = $cookie->value;
				update_option( self::OPTION_SESSION, $cookie->value, false );
				update_option( self::OPTION_SITE, $this->site_url, false );
				update_option( self::OPTION_PREFIX, $this->prefix, false );
				return true;
			}
		}

		return new WP_Error( 'mavicms_no_session', __( 'Signed in but no session cookie came back.', 'mavicms-migrator' ) );
	}

	/**
	 * Confirms the session is still valid.
	 *
	 * @return array|WP_Error Decoded response of GET /me.
	 */
	public function whoami() {
		return $this->request( 'GET', '/me' );
	}

	/**
	 * The languages content can be written in.
	 *
	 * @return array|WP_Error List of content languages.
	 */
	public function languages() {
		return $this->request( 'GET', '/languages' );
	}

	/**
	 * Adds a content language.
	 *
	 * @param string $code Language code.
	 * @param string $name Human-readable name.
	 * @return array|WP_Error
	 */
	public function create_language( $code, $name ) {
		return $this->request(
			'POST',
			'/languages',
			array(
				'code' => $code,
				'name' => $name,
			)
		);
	}

	/**
	 * Creates a category.
	 *
	 * @param array $payload Category fields.
	 * @return array|WP_Error
	 */
	public function create_category( array $payload ) {
		return $this->request( 'POST', '/categories', $payload );
	}

	/**
	 * Lists existing categories.
	 *
	 * @return array|WP_Error
	 */
	public function categories() {
		return $this->request( 'GET', '/categories' );
	}

	/**
	 * Creates a post.
	 *
	 * @param array $payload Post fields.
	 * @return array|WP_Error
	 */
	public function create_post( array $payload ) {
		return $this->request( 'POST', '/posts', $payload );
	}

	/**
	 * Finds a post already on the destination by its address.
	 *
	 * @param string $slug   Post address.
	 * @param string $locale Language it would be in.
	 * @return array|null|WP_Error The post, or null when there is none.
	 */
	public function find_post( $slug, $locale ) {
		$found = $this->request(
			'GET',
			'/posts?slug=' . rawurlencode( $slug ) . '&locale=' . rawurlencode( $locale )
		);
		if ( is_wp_error( $found ) ) {
			return $found;
		}
		// A listing is a page: `{ items, total, limit, offset }`.
		$items = isset( $found['items'] ) ? $found['items'] : $found;
		return isset( $items[0] ) ? $items[0] : null;
	}

	/**
	 * Joins a post to another post's translation group.
	 *
	 * @param string $id     Post id to move.
	 * @param string $target Post id whose group to join.
	 * @return array|WP_Error
	 */
	public function join_translation_group( $id, $target ) {
		return $this->request( 'PATCH', '/posts/' . rawurlencode( $id ) . '/translation-group', array( 'join' => $target ) );
	}

	/**
	 * Asks Mavi CMS to pull an image from a URL.
	 *
	 * @param string $url      Publicly reachable image URL.
	 * @param string $filename Original file name.
	 * @param string $alt_text Alternative text.
	 * @return array|WP_Error
	 */
	public function import_media( $url, $filename = '', $alt_text = '' ) {
		return $this->request(
			'POST',
			'/media/import',
			array(
				'url'      => $url,
				'filename' => $filename,
				'alt_text' => $alt_text,
			)
		);
	}

	/**
	 * Uploads a local file to the media library.
	 *
	 * The fallback for when Mavi CMS cannot reach this site — a WordPress
	 * behind a firewall, on a private network, or on the same machine — where
	 * asking it to fetch the image by URL can never work.
	 *
	 * @param string $path     Absolute path to the file.
	 * @param string $filename Name to store it under.
	 * @return array|WP_Error
	 */
	public function upload_media( $path, $filename ) {
		// phpcs:ignore WordPress.WP.AlternativeFunctions.file_get_contents_file_get_contents -- a local upload, not a remote request.
		$contents = file_get_contents( $path );
		if ( false === $contents ) {
			return new WP_Error( 'mavicms_unreadable', __( 'The file could not be read.', 'mavicms-migrator' ) );
		}

		$boundary = wp_generate_password( 24, false );
		$body     = '--' . $boundary . "\r\n"
			. 'Content-Disposition: form-data; name="file"; filename="' . $filename . '"' . "\r\n"
			. 'Content-Type: application/octet-stream' . "\r\n\r\n"
			. $contents . "\r\n"
			. '--' . $boundary . "--\r\n";

		$response = wp_remote_post(
			$this->endpoint( '/media' ),
			array(
				'timeout' => 120,
				'headers' => array(
					'Content-Type' => 'multipart/form-data; boundary=' . $boundary,
					'Cookie'       => 'mavicms_session=' . $this->session,
				),
				'body'    => $body,
			)
		);

		if ( is_wp_error( $response ) ) {
			return $response;
		}

		$code    = wp_remote_retrieve_response_code( $response );
		$decoded = json_decode( wp_remote_retrieve_body( $response ), true );
		if ( $code < 200 || $code > 299 ) {
			return new WP_Error( 'mavicms_upload_failed', $this->error_message( $response, __( 'The upload was refused.', 'mavicms-migrator' ) ) );
		}

		return is_array( $decoded ) ? $decoded : array();
	}

	/**
	 * Sends an authenticated request and unwraps the response.
	 *
	 * @param string     $method HTTP method.
	 * @param string     $path   API path beginning with a slash.
	 * @param array|null $body   Optional JSON body.
	 * @return array|WP_Error Decoded body, or an error.
	 */
	private function request( $method, $path, $body = null ) {
		if ( '' === $this->site_url ) {
			return new WP_Error( 'mavicms_not_configured', __( 'No Mavi CMS address is set.', 'mavicms-migrator' ) );
		}

		$args = array(
			'method'  => $method,
			// Importing an image makes Mavi CMS fetch it from this site, which
			// can take a while on a slow origin.
			'timeout' => 60,
			'headers' => array(
				'Content-Type' => 'application/json',
				'Cookie'       => 'mavicms_session=' . $this->session,
			),
		);
		if ( null !== $body ) {
			$args['body'] = wp_json_encode( $body );
		}

		$response = wp_remote_request( $this->endpoint( $path ), $args );
		if ( is_wp_error( $response ) ) {
			return $response;
		}

		$code    = wp_remote_retrieve_response_code( $response );
		$decoded = json_decode( wp_remote_retrieve_body( $response ), true );

		if ( 401 === $code ) {
			delete_option( self::OPTION_SESSION );
			return new WP_Error( 'mavicms_signed_out', __( 'The Mavi CMS session expired. Connect again.', 'mavicms-migrator' ) );
		}
		if ( $code < 200 || $code > 299 ) {
			return new WP_Error(
				'mavicms_http_' . $code,
				$this->error_message( $response, sprintf( /* translators: %d: HTTP status code. */ __( 'Mavi CMS returned %d.', 'mavicms-migrator' ), $code ) )
			);
		}

		return is_array( $decoded ) ? $decoded : array();
	}

	/**
	 * Turns an API path into a full URL.
	 *
	 * @param string $path API path.
	 * @return string Absolute URL.
	 */
	private function endpoint( $path ) {
		return $this->site_url . $this->prefix . $path;
	}

	/**
	 * Finds where the API lives under the given address.
	 *
	 * Asking rather than assuming means the same address works whether the
	 * user typed their site, which proxies the API under `/api`, or the
	 * backend's own address, which serves it at the root.
	 *
	 * @return string|WP_Error The prefix to use.
	 */
	private function detect_prefix() {
		foreach ( array( '/api', '' ) as $candidate ) {
			$response = wp_remote_get(
				$this->site_url . $candidate . '/setup/status',
				array( 'timeout' => 15 )
			);
			if ( is_wp_error( $response ) || 200 !== wp_remote_retrieve_response_code( $response ) ) {
				continue;
			}
			$decoded = json_decode( wp_remote_retrieve_body( $response ), true );
			if ( is_array( $decoded ) && array_key_exists( 'installed', $decoded ) ) {
				return $candidate;
			}
		}

		return new WP_Error(
			'mavicms_not_found',
			__( 'No Mavi CMS site answered at that address. Check the address and that the site is reachable from here.', 'mavicms-migrator' )
		);
	}

	/**
	 * Pulls the API's own error text out of a response when there is one.
	 *
	 * @param array  $response Raw wp_remote_* response.
	 * @param string $fallback Message to use when the body says nothing useful.
	 * @return string
	 */
	private function error_message( $response, $fallback ) {
		$decoded = json_decode( wp_remote_retrieve_body( $response ), true );
		if ( is_array( $decoded ) && ! empty( $decoded['error'] ) ) {
			return (string) $decoded['error'];
		}
		return $fallback;
	}
}
