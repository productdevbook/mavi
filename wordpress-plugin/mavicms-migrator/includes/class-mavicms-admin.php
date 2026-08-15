<?php
/**
 * Admin screen and AJAX handlers.
 *
 * @package MaviCMS_Migrator
 */

defined( 'ABSPATH' ) || exit;

/**
 * The "Migrate to Mavi CMS" screen under Tools.
 */
class MaviCMS_Admin {

	const CAPABILITY = 'manage_options';
	const NONCE      = 'mavicms_migrator';

	/**
	 * Registers hooks.
	 */
	public function register() {
		add_action( 'admin_menu', array( $this, 'add_page' ) );
		add_action( 'wp_ajax_mavicms_connect', array( $this, 'ajax_connect' ) );
		add_action( 'wp_ajax_mavicms_migrate_post', array( $this, 'ajax_migrate_post' ) );
		add_action( 'wp_ajax_mavicms_link_translations', array( $this, 'ajax_link_translations' ) );
		add_action( 'wp_ajax_mavicms_set_locale', array( $this, 'ajax_set_locale' ) );
		add_action( 'wp_ajax_mavicms_reset', array( $this, 'ajax_reset' ) );
	}

	/**
	 * Adds the Tools submenu entry.
	 */
	public function add_page() {
		add_management_page(
			__( 'Migrate to Mavi CMS', 'mavicms-migrator' ),
			__( 'Migrate to Mavi CMS', 'mavicms-migrator' ),
			self::CAPABILITY,
			'mavicms-migrator',
			array( $this, 'render' )
		);
	}

	/**
	 * Renders the screen.
	 */
	public function render() {
		if ( ! current_user_can( self::CAPABILITY ) ) {
			wp_die( esc_html__( 'You do not have permission to view this page.', 'mavicms-migrator' ) );
		}

		$client    = new MaviCMS_Client();
		$migrator  = new MaviCMS_Migrator( $client );
		$connected = $client->has_session();
		$progress  = $migrator->progress();
		$breakdown = $migrator->language_breakdown();

		// Filled in here rather than left to the script, so returning to the
		// page with a stored session does not show an empty language list.
		$languages = array();
		$locale    = (string) get_option( MaviCMS_Migrator::OPTION_DEFAULT_LOCALE, '' );
		if ( $connected ) {
			$fetched = $client->languages();
			if ( is_wp_error( $fetched ) ) {
				$connected = false;
			} else {
				$languages = $fetched;
			}
		}

		require MAVICMS_MIGRATOR_DIR . 'views/admin-page.php';
	}

	/**
	 * Signs in to Mavi CMS and remembers the session.
	 */
	public function ajax_connect() {
		$this->guard();

		// phpcs:disable WordPress.Security.NonceVerification.Missing -- guard() verifies the nonce.
		$site     = isset( $_POST['site'] ) ? esc_url_raw( wp_unslash( $_POST['site'] ) ) : '';
		$username = isset( $_POST['username'] ) ? sanitize_text_field( wp_unslash( $_POST['username'] ) ) : '';
		// Not sanitized: a password is an opaque secret and filtering it would
		// silently change what the user typed. It is sent onward and dropped.
		// phpcs:ignore WordPress.Security.ValidatedSanitizedInput.InputNotSanitized
		$password = isset( $_POST['password'] ) ? (string) wp_unslash( $_POST['password'] ) : '';
		$locale   = isset( $_POST['locale'] ) ? sanitize_text_field( wp_unslash( $_POST['locale'] ) ) : '';
		// phpcs:enable WordPress.Security.NonceVerification.Missing

		if ( '' === $site || '' === $username || '' === $password ) {
			wp_send_json_error( array( 'message' => __( 'Address, username and password are all required.', 'mavicms-migrator' ) ) );
		}

		$client = new MaviCMS_Client( $site );
		$result = $client->connect( $username, $password );
		if ( is_wp_error( $result ) ) {
			wp_send_json_error( array( 'message' => $result->get_error_message() ) );
		}

		$languages = $client->languages();
		if ( is_wp_error( $languages ) ) {
			wp_send_json_error( array( 'message' => $languages->get_error_message() ) );
		}

		// A post in a language the destination has never heard of is refused
		// outright, so the languages this site actually uses are created up
		// front rather than failing one post at a time.
		$created = $this->create_missing_languages( $client, wp_list_pluck( $languages, 'code' ) );
		if ( $created ) {
			$languages = $client->languages();
			if ( is_wp_error( $languages ) ) {
				wp_send_json_error( array( 'message' => $languages->get_error_message() ) );
			}
		}

		$codes = wp_list_pluck( $languages, 'code' );
		if ( '' === $locale || ! in_array( $locale, $codes, true ) ) {
			$locale = '';
			foreach ( $languages as $language ) {
				if ( ! empty( $language['is_default'] ) ) {
					$locale = $language['code'];
					break;
				}
			}
			if ( '' === $locale && $codes ) {
				$locale = $codes[0];
			}
		}
		update_option( MaviCMS_Migrator::OPTION_DEFAULT_LOCALE, $locale, false );

		wp_send_json_success(
			array(
				'languages' => $languages,
				'locale'    => $locale,
				'message'   => __( 'Connected.', 'mavicms-migrator' ),
			)
		);
	}

	/**
	 * Migrates a single post. The browser drives the loop so a large archive
	 * never depends on one long-running request.
	 */
	public function ajax_migrate_post() {
		$this->guard();

		// phpcs:ignore WordPress.Security.NonceVerification.Missing -- guard() verifies the nonce.
		$post_id = isset( $_POST['post_id'] ) ? absint( wp_unslash( $_POST['post_id'] ) ) : 0;
		if ( ! $post_id ) {
			wp_send_json_error( array( 'message' => __( 'No post given.', 'mavicms-migrator' ) ) );
		}

		$migrator = new MaviCMS_Migrator( new MaviCMS_Client() );
		$result   = $migrator->migrate_post( $post_id );

		wp_send_json_success( $result );
	}

	/**
	 * Stores the fallback language.
	 *
	 * Its own endpoint so that changing the language does not mean signing in
	 * again — the password field is cleared after connecting, so reconnecting
	 * to change one dropdown would mean typing it out afresh.
	 */
	public function ajax_set_locale() {
		$this->guard();

		// phpcs:ignore WordPress.Security.NonceVerification.Missing -- guard() verifies the nonce.
		$locale = isset( $_POST['locale'] ) ? sanitize_text_field( wp_unslash( $_POST['locale'] ) ) : '';
		if ( '' === $locale ) {
			wp_send_json_error( array( 'message' => __( 'No language given.', 'mavicms-migrator' ) ) );
		}

		$languages = ( new MaviCMS_Client() )->languages();
		if ( is_wp_error( $languages ) ) {
			wp_send_json_error( array( 'message' => $languages->get_error_message() ) );
		}
		if ( ! in_array( $locale, wp_list_pluck( $languages, 'code' ), true ) ) {
			wp_send_json_error( array( 'message' => __( 'That language does not exist on the destination.', 'mavicms-migrator' ) ) );
		}

		update_option( MaviCMS_Migrator::OPTION_DEFAULT_LOCALE, $locale, false );
		wp_send_json_success( array( 'locale' => $locale ) );
	}

	/**
	 * Runs the translation-linking pass.
	 */
	public function ajax_link_translations() {
		$this->guard();

		$migrator = new MaviCMS_Migrator( new MaviCMS_Client() );
		wp_send_json_success( $migrator->link_translations() );
	}

	/**
	 * Clears what the plugin remembers so a migration can start over.
	 */
	public function ajax_reset() {
		$this->guard();

		$migrator = new MaviCMS_Migrator( new MaviCMS_Client() );
		$migrator->reset();

		wp_send_json_success( array( 'message' => __( 'Migration history cleared.', 'mavicms-migrator' ) ) );
	}

	/**
	 * Adds any language this site uses that the destination is missing.
	 *
	 * @param MaviCMS_Client $client   Connected client.
	 * @param string[]       $existing Codes the destination already has.
	 * @return string[] Codes that were created.
	 */
	private function create_missing_languages( MaviCMS_Client $client, array $existing ) {
		$wanted = array();

		if ( function_exists( 'pll_languages_list' ) ) {
			foreach ( pll_languages_list( array( 'fields' => '' ) ) as $language ) {
				$wanted[ $language->slug ] = $language->name;
			}
		} elseif ( defined( 'ICL_SITEPRESS_VERSION' ) ) {
			$active = apply_filters( 'wpml_active_languages', null );
			if ( is_array( $active ) ) {
				foreach ( $active as $code => $language ) {
					$wanted[ $code ] = isset( $language['native_name'] ) ? $language['native_name'] : $code;
				}
			}
		}

		$created = array();
		foreach ( $wanted as $code => $name ) {
			if ( in_array( $code, $existing, true ) ) {
				continue;
			}
			$result = $client->create_language( $code, $name );
			if ( ! is_wp_error( $result ) ) {
				$created[] = $code;
			}
		}

		return $created;
	}

	/**
	 * Refuses anyone without the capability or a valid nonce.
	 *
	 * Every AJAX handler calls this first; the `phpcs:ignore` comments around
	 * the `$_POST` reads point back here, since the sniffer cannot see a nonce
	 * check made through a helper.
	 */
	private function guard() {
		if ( ! current_user_can( self::CAPABILITY ) ) {
			wp_send_json_error( array( 'message' => __( 'You do not have permission to do this.', 'mavicms-migrator' ) ), 403 );
		}
		check_ajax_referer( self::NONCE, 'nonce' );
	}
}
