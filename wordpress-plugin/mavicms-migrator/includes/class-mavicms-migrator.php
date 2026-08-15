<?php
/**
 * Moves WordPress content into Mavi CMS.
 *
 * @package MaviCMS_Migrator
 */

defined( 'ABSPATH' ) || exit;

/**
 * Migrates posts, taxonomies and media in resumable batches.
 *
 * Every WordPress id that has been sent is recorded, so a run that stops
 * halfway — a timeout, a closed browser tab, an expired session — can be
 * started again without creating duplicates.
 */
class MaviCMS_Migrator {

	const OPTION_MAP_POSTS      = 'mavicms_map_posts';
	const OPTION_MAP_TERMS      = 'mavicms_map_terms';
	const OPTION_MAP_MEDIA      = 'mavicms_map_media';
	const OPTION_DEFAULT_LOCALE = 'mavicms_default_locale';

	/**
	 * API client for the destination site.
	 *
	 * @var MaviCMS_Client
	 */
	private $client;

	/**
	 * Things that went wrong during the current post but did not stop it.
	 *
	 * @var string[]
	 */
	private $warnings = array();

	/**
	 * Language codes known to exist on the destination, once looked up.
	 *
	 * @var string[]|null
	 */
	private $languages = null;

	/**
	 * Builds a migrator around a connected client.
	 *
	 * @param MaviCMS_Client $client Configured API client.
	 */
	public function __construct( MaviCMS_Client $client ) {
		$this->client = $client;
	}

	/**
	 * Everything that will be migrated, in a stable order.
	 *
	 * @return int[] Ids of published and draft posts, oldest first.
	 */
	public function post_ids() {
		// WPML narrows admin queries to the language being edited, and does not
		// answer to a query argument, so it has to be switched and switched
		// back around the call.
		$wpml = defined( 'ICL_SITEPRESS_VERSION' );
		if ( $wpml ) {
			do_action( 'wpml_switch_language', 'all' );
		}

		$ids = get_posts(
			array(
				'post_type'        => 'post',
				'post_status'      => array( 'publish', 'draft', 'pending', 'future', 'private' ),
				'numberposts'      => -1,
				'orderby'          => 'ID',
				'order'            => 'ASC',
				'fields'           => 'ids',
				// Polylang filters admin queries down to the language currently
				// selected in the admin bar, and `suppress_filters` does not
				// stop it: it hooks the query, not the SQL. Without this an
				// English archive is invisible to the migration and simply
				// never gets sent.
				'lang'             => '',
				'suppress_filters' => true,
			)
		);

		if ( $wpml ) {
			do_action( 'wpml_switch_language', null );
		}

		sort( $ids );
		return $ids;
	}

	/**
	 * How far the migration has got.
	 *
	 * @return array{total:int,done:int,failed:int} Progress so far.
	 */
	public function progress() {
		$map = $this->map( self::OPTION_MAP_POSTS );
		$ids = $this->post_ids();
		return array(
			'total'  => count( $ids ),
			'done'   => count( array_intersect( $ids, array_keys( $map ) ) ),
			'failed' => 0,
		);
	}

	/**
	 * How many posts will go to each language, worked out the same way the
	 * migration itself works it out.
	 *
	 * Shown before anything is sent, because "why did my English posts end up
	 * in Turkish" is otherwise only answerable after the fact: a post that no
	 * multilingual plugin has tagged has no language of its own, and there is
	 * no way to tell from the post list.
	 *
	 * @return array<string,int> Language code (or '' for untagged) to count.
	 */
	public function language_breakdown() {
		$counts = array();
		foreach ( $this->post_ids() as $post_id ) {
			$code            = $this->post_language( (int) $post_id );
			$counts[ $code ] = isset( $counts[ $code ] ) ? $counts[ $code ] + 1 : 1;
		}
		ksort( $counts );
		return $counts;
	}

	/**
	 * Forgets what has been migrated. The content already in Mavi CMS stays.
	 */
	public function reset() {
		delete_option( self::OPTION_MAP_POSTS );
		delete_option( self::OPTION_MAP_TERMS );
		delete_option( self::OPTION_MAP_MEDIA );
	}

	/**
	 * Migrates one post: its category, tags, featured image and inline images.
	 *
	 * @param int $post_id WordPress post id.
	 * @return array{status:string,message:string}
	 */
	public function migrate_post( $post_id ) {
		$posts_map = $this->map( self::OPTION_MAP_POSTS );
		if ( isset( $posts_map[ $post_id ] ) ) {
			return array(
				'status'  => 'skipped',
				'message' => __( 'Already migrated.', 'mavicms-migrator' ),
			);
		}

		$this->warnings = array();

		$post = get_post( $post_id );
		if ( ! $post ) {
			return array(
				'status'  => 'failed',
				'message' => __( 'Post not found.', 'mavicms-migrator' ),
			);
		}

		$locale = $this->locale_of( $post_id );
		// The destination refuses a post in a language it does not have. Adding
		// it here rather than only when connecting means a language added to
		// WordPress afterwards does not fail every post that uses it.
		$this->ensure_language( $locale );

		$category_ids = array();
		foreach ( wp_get_post_categories( $post_id ) as $term_id ) {
			$mapped = $this->migrate_category( $term_id, $locale );
			if ( is_wp_error( $mapped ) ) {
				return array(
					'status'  => 'failed',
					'message' => $mapped->get_error_message(),
				);
			}
			// WordPress puts everything in "Uncategorized" by default; carrying
			// that across would categorise the whole archive under a word the
			// author never chose.
			if ( '' !== $mapped ) {
				$category_ids[] = $mapped;
			}
		}

		$tags = array();
		foreach ( wp_get_post_tags( $post_id ) as $tag ) {
			$tags[] = $tag->name;
		}

		$content = $this->rewrite_images( $this->render( (string) $post->post_content ) );
		if ( is_wp_error( $content ) ) {
			return array(
				'status'  => 'failed',
				'message' => $content->get_error_message(),
			);
		}

		$cover_url = '';
		$thumbnail = get_post_thumbnail_id( $post_id );
		if ( $thumbnail ) {
			$media = $this->migrate_attachment( (int) $thumbnail );
			if ( is_wp_error( $media ) ) {
				$this->warnings[] = sprintf(
					/* translators: %s: reason the featured image failed. */
					__( 'the featured image was left behind (%s)', 'mavicms-migrator' ),
					$media->get_error_message()
				);
			} else {
				$cover_url = $media;
			}
		}

		$payload = array(
			'title'          => $post->post_title,
			'slug'           => $post->post_name ? $post->post_name : sanitize_title( $post->post_title ),
			'excerpt'        => (string) $post->post_excerpt,
			'status'         => $this->status_of( $post ),
			'content_html'   => $content,
			'category_ids'   => $category_ids,
			'tags'           => $tags,
			'cover_url'      => $cover_url,
			'author'         => (string) get_the_author_meta( 'display_name', (int) $post->post_author ),
			'locale'         => $locale,
			'created_at'     => $this->utc( $post->post_date_gmt, $post->post_date ),
			'publish_at'     => $this->utc( $post->post_date_gmt, $post->post_date ),
			'allow_comments' => 'closed' !== $post->comment_status,
		);

		// Link as we go: if a translation of this post has already been sent,
		// join its group now. Leaving it all to the pass at the end means an
		// interrupted run ends with everything it did send unlinked.
		$sibling = $this->migrated_sibling( $post_id );
		if ( $sibling ) {
			$payload['translation_of'] = $sibling;
		}

		$created = $this->client->create_post( $payload );
		if ( is_wp_error( $created ) && $sibling ) {
			// The group may already hold this language — from an earlier run,
			// or two WordPress posts claiming the same translation. Send it
			// unlinked rather than losing the post; the pass at the end sorts
			// out what it can.
			unset( $payload['translation_of'] );
			$created = $this->client->create_post( $payload );
		}
		if ( is_wp_error( $created ) ) {
			// It may simply be there already, from a run whose record of what
			// it had sent was cleared. Adopt it rather than reporting a
			// conflict the user can do nothing useful about.
			$existing = $this->client->find_post( $payload['slug'], $locale );
			if ( is_wp_error( $existing ) || ! $existing ) {
				return array(
					'status'  => 'failed',
					'message' => $created->get_error_message(),
				);
			}

			$posts_map             = $this->map( self::OPTION_MAP_POSTS );
			$posts_map[ $post_id ] = $existing['id'];
			$this->save_map( self::OPTION_MAP_POSTS, $posts_map );

			return array(
				'status'  => 'skipped',
				'message' => sprintf(
					/* translators: %s: post title. */
					__( '"%s" was already on the destination.', 'mavicms-migrator' ),
					$post->post_title
				),
			);
		}

		$posts_map             = $this->map( self::OPTION_MAP_POSTS );
		$posts_map[ $post_id ] = $created['id'];
		$this->save_map( self::OPTION_MAP_POSTS, $posts_map );

		$message = sprintf(
			/* translators: 1: post title, 2: language it was migrated into. */
			__( 'Migrated "%1$s" into %2$s.', 'mavicms-migrator' ),
			$post->post_title,
			// The language the destination actually stored, not the one asked
			// for: an empty request means "the site default", and reporting
			// that back as blank helps nobody.
			isset( $created['locale'] ) ? $created['locale'] : $locale
		);
		if ( $this->warnings ) {
			$message .= ' ' . implode( '; ', $this->warnings ) . '.';
		}

		return array(
			'status'  => $this->warnings ? 'warned' : 'migrated',
			'message' => $message,
		);
	}

	/**
	 * The destination id of a translation of this post that has already been
	 * migrated, if there is one.
	 *
	 * @param int $post_id WordPress post id.
	 * @return string|null
	 */
	private function migrated_sibling( $post_id ) {
		$posts_map = $this->map( self::OPTION_MAP_POSTS );
		foreach ( $this->translation_group_of( $post_id ) as $sibling_id ) {
			if ( (int) $sibling_id !== (int) $post_id && isset( $posts_map[ $sibling_id ] ) ) {
				return $posts_map[ $sibling_id ];
			}
		}
		return null;
	}

	/**
	 * Links translations once every post exists.
	 *
	 * Runs as a second pass because a translation can only join a group whose
	 * other side has already been created.
	 *
	 * @return array{linked:int,skipped:int}
	 */
	public function link_translations() {
		$posts_map = $this->map( self::OPTION_MAP_POSTS );
		$linked    = 0;
		$skipped   = 0;
		$seen      = array();

		foreach ( array_keys( $posts_map ) as $post_id ) {
			$group = $this->translation_group_of( (int) $post_id );
			if ( count( $group ) < 2 ) {
				continue;
			}

			$anchor = null;
			foreach ( $group as $sibling_id ) {
				if ( ! isset( $posts_map[ $sibling_id ] ) || isset( $seen[ $sibling_id ] ) ) {
					continue;
				}
				$seen[ $sibling_id ] = true;
				if ( null === $anchor ) {
					$anchor = $posts_map[ $sibling_id ];
					continue;
				}
				$result = $this->client->join_translation_group( $posts_map[ $sibling_id ], $anchor );
				if ( is_wp_error( $result ) ) {
					++$skipped;
					continue;
				}
				++$linked;
			}
		}

		return array(
			'linked'  => $linked,
			'skipped' => $skipped,
		);
	}

	/**
	 * Creates the category in Mavi CMS, parents first.
	 *
	 * @param int    $term_id WordPress term id.
	 * @param string $locale  Content language for the new category.
	 * @return string|WP_Error Mavi CMS category id, or '' for "Uncategorized".
	 */
	private function migrate_category( $term_id, $locale ) {
		$term = get_term( $term_id, 'category' );
		if ( ! $term || is_wp_error( $term ) ) {
			return '';
		}
		if ( 'uncategorized' === $term->slug ) {
			return '';
		}

		$key   = $locale . ':' . $term_id;
		$terms = $this->map( self::OPTION_MAP_TERMS );
		if ( isset( $terms[ $key ] ) ) {
			return $terms[ $key ];
		}

		$parent_id = '';
		if ( $term->parent ) {
			$parent = $this->migrate_category( (int) $term->parent, $locale );
			if ( is_wp_error( $parent ) ) {
				return $parent;
			}
			$parent_id = $parent;
		}

		$payload = array(
			'name'        => $term->name,
			'description' => (string) $term->description,
			'locale'      => $locale,
		);
		if ( '' !== $parent_id ) {
			$payload['parent_id'] = $parent_id;
		}

		$created = $this->client->create_category( $payload );
		if ( is_wp_error( $created ) ) {
			return $created;
		}

		$terms         = $this->map( self::OPTION_MAP_TERMS );
		$terms[ $key ] = $created['id'];
		$this->save_map( self::OPTION_MAP_TERMS, $terms );

		return $created['id'];
	}

	/**
	 * Hands an attachment to Mavi CMS, which fetches it from this site.
	 *
	 * @param int $attachment_id WordPress attachment id.
	 * @return string|WP_Error New URL on the Mavi CMS side.
	 */
	private function migrate_attachment( $attachment_id ) {
		$source = wp_get_attachment_url( $attachment_id );
		if ( ! $source ) {
			return new WP_Error( 'mavicms_no_attachment', __( 'The attachment has no URL.', 'mavicms-migrator' ) );
		}
		return $this->migrate_image( $source, (int) $attachment_id );
	}

	/**
	 * Copies one image across by its address, whether or not WordPress still
	 * has a media record for it.
	 *
	 * Keyed by address rather than attachment id: page builders and pasted
	 * markup point at files whose attachment row is missing or unmatchable,
	 * and refusing to move those was leaving posts loading their pictures from
	 * the site they had just left.
	 *
	 * @param string   $source        Absolute URL of the image.
	 * @param int|null $attachment_id Its attachment, when there is one — used
	 *                                for the alt text and the local file.
	 * @return string|WP_Error New URL on the Mavi CMS side.
	 */
	private function migrate_image( $source, $attachment_id = null ) {
		$media = $this->map( self::OPTION_MAP_MEDIA );

		// One entry per image, not per generated size: "photo-300x200.jpg" and
		// "photo.jpg" are the same picture and must not be copied twice.
		$key = $this->strip_size_suffix( $source );
		if ( isset( $media[ $key ] ) ) {
			return $media[ $key ];
		}
		// Earlier versions keyed this map by attachment id; honour those so an
		// upgrade mid-migration does not re-upload what is already across.
		if ( $attachment_id && isset( $media[ $attachment_id ] ) ) {
			return $media[ $attachment_id ];
		}

		// Prefer the original over whichever generated size the content
		// happened to reference.
		if ( $attachment_id ) {
			$full = wp_get_attachment_url( $attachment_id );
			if ( $full ) {
				$source = $full;
			}
		}

		$filename = basename( (string) wp_parse_url( $source, PHP_URL_PATH ) );
		$alt      = $attachment_id
			? (string) get_post_meta( $attachment_id, '_wp_attachment_image_alt', true )
			: '';

		$imported = $this->client->import_media( $source, $filename, $alt );
		if ( is_wp_error( $imported ) ) {
			// Mavi CMS could not reach this site — behind a firewall, on a
			// private network, or on the same machine. Push the bytes instead
			// of asking it to pull them.
			$path = $this->local_path( $source, $attachment_id );
			if ( ! $path ) {
				return $imported;
			}
			$imported = $this->client->upload_media( $path, $filename );
			if ( is_wp_error( $imported ) ) {
				return $imported;
			}
		}

		$url = $this->absolute_media_url( $imported['url'] );

		$media         = $this->map( self::OPTION_MAP_MEDIA );
		$media[ $key ] = $url;
		$this->save_map( self::OPTION_MAP_MEDIA, $media );

		return $url;
	}

	/**
	 * Where the file sits on disk, from its attachment or worked out from the
	 * uploads directory.
	 *
	 * @param string   $source        Absolute URL of the image.
	 * @param int|null $attachment_id Its attachment, when there is one.
	 * @return string|null
	 */
	private function local_path( $source, $attachment_id ) {
		if ( $attachment_id ) {
			$path = get_attached_file( $attachment_id );
			if ( $path && file_exists( $path ) ) {
				return $path;
			}
		}

		$uploads = wp_get_upload_dir();
		if ( empty( $uploads['baseurl'] ) || empty( $uploads['basedir'] ) ) {
			return null;
		}
		if ( 0 !== strpos( $source, $uploads['baseurl'] ) ) {
			return null;
		}

		$relative = ltrim( substr( $source, strlen( $uploads['baseurl'] ) ), '/' );
		// No traversal out of the uploads directory, whatever the content says.
		if ( '' === $relative || false !== strpos( $relative, '..' ) ) {
			return null;
		}
		$path = $uploads['basedir'] . '/' . $relative;
		return file_exists( $path ) ? $path : null;
	}

	/**
	 * Turns stored post content into the HTML the site actually showed.
	 *
	 * WordPress keeps paragraphs as blank lines and leaves shortcodes
	 * unexpanded, then renders both on the way out. Sending the stored text
	 * as-is produces one enormous run-on block, and any picture a builder
	 * placed through a shortcode never appears at all.
	 *
	 * The full `the_content` filter is deliberately not used: it invites every
	 * plugin on the site to staple related-post lists and share buttons onto
	 * the text being migrated.
	 *
	 * @param string $content Raw post content.
	 * @return string
	 */
	private function render( $content ) {
		return wpautop( do_shortcode( $content ) );
	}

	/**
	 * Repoints every uploaded image in the content at its Mavi CMS copy.
	 *
	 * Images that fail to transfer keep their original address rather than
	 * breaking the post: a picture still served by the old site beats a broken
	 * one, and the failure is visible in the log.
	 *
	 * @param string $content Post content.
	 * @return string
	 */
	private function rewrite_images( $content ) {
		$uploads = wp_get_upload_dir();
		$base    = isset( $uploads['baseurl'] ) ? $uploads['baseurl'] : '';
		if ( '' === $base ) {
			return $content;
		}

		// Every address into this site's uploads, not just the one in `src`:
		// WordPress also writes each generated size into `srcset` and usually
		// links the full-size file from an enclosing `<a href>`. Rewriting only
		// `src` leaves a post that looks migrated but still loads half its
		// bytes from the old site.
		$pattern = '#' . preg_quote( $base, '#' ) . '[^\s"\'<>\\\\)]+#i';
		if ( ! preg_match_all( $pattern, $content, $matches ) ) {
			return $content;
		}

		$replacements = array();
		foreach ( array_unique( $matches[0] ) as $source ) {
			// The attachment is a nicety, not a requirement: it supplies the alt
			// text and a local file to fall back on, and plenty of images in
			// real content have no matchable attachment row at all.
			$attachment_id = attachment_url_to_postid( $this->strip_size_suffix( $source ) );
			$new           = $this->migrate_image( $source, $attachment_id ? (int) $attachment_id : null );
			if ( is_wp_error( $new ) ) {
				$this->warnings[] = sprintf(
					/* translators: 1: image file name, 2: reason it failed. */
					__( '%1$s still points at the old site (%2$s)', 'mavicms-migrator' ),
					basename( $source ),
					$new->get_error_message()
				);
				continue;
			}
			$replacements[ $source ] = $new;
		}

		if ( ! $replacements ) {
			return $content;
		}

		// Longest first: `photo-300x200.jpg` must be replaced before `photo.jpg`,
		// or the shorter match rewrites the start of the longer one and leaves
		// `-300x200.jpg` stuck on the end.
		uksort(
			$replacements,
			static function ( $a, $b ) {
				return strlen( $b ) - strlen( $a );
			}
		);
		$content = str_replace( array_keys( $replacements ), array_values( $replacements ), $content );

		// One file replaces the whole set of generated sizes, so a srcset of
		// identical candidates is just noise for the browser to weigh.
		return preg_replace( '/\s(?:srcset|sizes)=(["\'])(?:(?!\1).)*\1/i', '', $content );
	}

	/**
	 * Turns `photo-800x600.jpg` back into `photo.jpg`.
	 *
	 * Content usually points at a generated size, which is not an attachment of
	 * its own and would not resolve to an id.
	 *
	 * @param string $url Image URL.
	 * @return string
	 */
	private function strip_size_suffix( $url ) {
		return preg_replace( '/-\d+x\d+(\.[a-zA-Z0-9]+)$/', '$1', $url );
	}

	/**
	 * Makes a media URL absolute, whatever storage backend produced it.
	 *
	 * @param string $path URL or path returned by the media API.
	 * @return string Absolute URL usable from anywhere.
	 */
	private function absolute_media_url( $path ) {
		if ( preg_match( '#^https?://#i', $path ) ) {
			return $path;
		}
		return $this->client->site_url() . $path;
	}

	/**
	 * Maps a WordPress post status onto the Mavi CMS one.
	 *
	 * @param WP_Post $post Post being migrated.
	 * @return string Mavi CMS status.
	 */
	private function status_of( $post ) {
		switch ( $post->post_status ) {
			case 'publish':
				return 'published';
			case 'future':
				return 'scheduled';
			case 'pending':
				return 'review';
			default:
				return 'draft';
		}
	}

	/**
	 * Normalises a WordPress timestamp to UTC.
	 *
	 * @param string $gmt   GMT timestamp, which WordPress sometimes leaves empty.
	 * @param string $local Site-local timestamp.
	 * @return string RFC 3339 timestamp in UTC.
	 */
	private function utc( $gmt, $local ) {
		$value = ( $gmt && '0000-00-00 00:00:00' !== $gmt ) ? $gmt : get_gmt_from_date( $local );
		return gmdate( 'Y-m-d\TH:i:s\Z', strtotime( $value . ' UTC' ) );
	}

	/**
	 * Content language of a post, from Polylang or WPML when either is active.
	 *
	 * @param int $post_id WordPress post id.
	 * @return string Language code, or the configured default.
	 */
	private function locale_of( $post_id ) {
		$code = $this->post_language( $post_id );
		return '' !== $code ? $code : (string) get_option( self::OPTION_DEFAULT_LOCALE, '' );
	}

	/**
	 * Creates the language on the destination if it does not have it yet.
	 *
	 * @param string $code Language code.
	 */
	private function ensure_language( $code ) {
		if ( '' === $code ) {
			return;
		}
		if ( null === $this->languages ) {
			$languages       = $this->client->languages();
			$this->languages = is_wp_error( $languages ) ? array() : wp_list_pluck( $languages, 'code' );
		}
		if ( in_array( $code, $this->languages, true ) ) {
			return;
		}

		$result = $this->client->create_language( $code, $this->language_name( $code ) );
		if ( ! is_wp_error( $result ) ) {
			$this->languages[] = $code;
		}
	}

	/**
	 * What this site calls the language, so the destination does not end up
	 * listing bare codes.
	 *
	 * @param string $code Language code.
	 * @return string
	 */
	private function language_name( $code ) {
		if ( function_exists( 'pll_languages_list' ) ) {
			foreach ( pll_languages_list( array( 'fields' => '' ) ) as $language ) {
				if ( $language->slug === $code ) {
					return $language->name;
				}
			}
		}
		if ( defined( 'ICL_SITEPRESS_VERSION' ) ) {
			$active = apply_filters( 'wpml_active_languages', null );
			if ( is_array( $active ) && isset( $active[ $code ]['native_name'] ) ) {
				return $active[ $code ]['native_name'];
			}
		}
		return $code;
	}

	/**
	 * The post's own language, or an empty string when nothing has tagged it.
	 *
	 * @param int $post_id WordPress post id.
	 * @return string
	 */
	private function post_language( $post_id ) {
		if ( function_exists( 'pll_get_post_language' ) ) {
			$code = pll_get_post_language( $post_id, 'slug' );
			if ( $code ) {
				return $code;
			}
		}

		if ( defined( 'ICL_SITEPRESS_VERSION' ) ) {
			$details = apply_filters(
				'wpml_post_language_details',
				null,
				$post_id
			);
			if ( is_array( $details ) && ! empty( $details['language_code'] ) ) {
				return $details['language_code'];
			}
		}

		return '';
	}

	/**
	 * Every post that is a translation of the given one, itself included.
	 *
	 * @param int $post_id WordPress post id.
	 * @return int[]
	 */
	private function translation_group_of( $post_id ) {
		if ( function_exists( 'pll_get_post_translations' ) ) {
			return array_map( 'intval', array_values( pll_get_post_translations( $post_id ) ) );
		}

		if ( defined( 'ICL_SITEPRESS_VERSION' ) ) {
			$trid = apply_filters( 'wpml_element_trid', null, $post_id, 'post_post' );
			if ( $trid ) {
				$translations = apply_filters( 'wpml_get_element_translations', null, $trid, 'post_post' );
				if ( is_array( $translations ) ) {
					return array_map(
						static function ( $translation ) {
							return (int) $translation->element_id;
						},
						array_values( $translations )
					);
				}
			}
		}

		return array();
	}

	/**
	 * Reads one of the id maps.
	 *
	 * @param string $option Option name.
	 * @return array
	 */
	private function map( $option ) {
		$value = get_option( $option, array() );
		return is_array( $value ) ? $value : array();
	}

	/**
	 * Writes one of the id maps.
	 *
	 * @param string $option Option name.
	 * @param array  $value  Map to store.
	 */
	private function save_map( $option, array $value ) {
		update_option( $option, $value, false );
	}
}
