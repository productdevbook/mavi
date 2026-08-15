<?php
/**
 * Plugin Name:       Migrate to Mavi CMS
 * Plugin URI:        https://github.com/productdevbook/mavicms
 * Description:       Sends your WordPress posts, categories, tags and images to a Mavi CMS site.
 * Version:           0.2.0
 * Requires at least: 5.9
 * Requires PHP:      7.4
 * Author:            productdevbook
 * License:           GPL-2.0-or-later
 * License URI:       https://www.gnu.org/licenses/gpl-2.0.html
 * Text Domain:       mavicms-migrator
 * Domain Path:       /languages
 *
 * @package MaviCMS_Migrator
 */

defined( 'ABSPATH' ) || exit;

define( 'MAVICMS_MIGRATOR_VERSION', '0.2.0' );
define( 'MAVICMS_MIGRATOR_DIR', plugin_dir_path( __FILE__ ) );
define( 'MAVICMS_MIGRATOR_URL', plugin_dir_url( __FILE__ ) );

require_once MAVICMS_MIGRATOR_DIR . 'includes/class-mavicms-client.php';
require_once MAVICMS_MIGRATOR_DIR . 'includes/class-mavicms-migrator.php';
require_once MAVICMS_MIGRATOR_DIR . 'includes/class-mavicms-admin.php';

add_action(
	'plugins_loaded',
	static function () {
		load_plugin_textdomain( 'mavicms-migrator', false, dirname( plugin_basename( __FILE__ ) ) . '/languages' );
		( new MaviCMS_Admin() )->register();
	}
);

add_action(
	'admin_enqueue_scripts',
	static function ( $hook ) {
		if ( 'tools_page_mavicms-migrator' !== $hook ) {
			return;
		}

		wp_enqueue_style(
			'mavicms-migrator',
			MAVICMS_MIGRATOR_URL . 'assets/admin.css',
			array(),
			MAVICMS_MIGRATOR_VERSION
		);
		wp_enqueue_script(
			'mavicms-migrator',
			MAVICMS_MIGRATOR_URL . 'assets/admin.js',
			array(),
			MAVICMS_MIGRATOR_VERSION,
			true
		);

		$migrator = new MaviCMS_Migrator( new MaviCMS_Client() );
		wp_localize_script(
			'mavicms-migrator',
			'MaviCMSMigrator',
			array(
				'ajaxUrl' => admin_url( 'admin-ajax.php' ),
				'nonce'   => wp_create_nonce( MaviCMS_Admin::NONCE ),
				'postIds' => $migrator->post_ids(),
				'i18n'    => array(
					'connecting'    => __( 'Connecting…', 'mavicms-migrator' ),
					/* translators: 1: number of posts done, 2: total number of posts. */
					'progress'      => __( '%1$d of %2$d posts done.', 'mavicms-migrator' ),
					'languageSaved' => __( 'Language saved.', 'mavicms-migrator' ),
					'linking'       => __( 'Linking translations…', 'mavicms-migrator' ),
					/* translators: %d: number of linked translations. */
					'linked'        => __( 'Linked %d translations.', 'mavicms-migrator' ),
					'done'          => __( 'Migration finished.', 'mavicms-migrator' ),
					/* translators: 1: posts sent, 2: posts skipped, 3: posts that failed. */
					'summary'       => __( '%1$d sent, %2$d skipped as already migrated, %3$d failed.', 'mavicms-migrator' ),
					'allSkipped'    => __( 'Nothing new was sent — every post had been migrated before. To send them again, for instance into a different language, use "Forget history" first and remove the earlier copies from Mavi CMS, or they will be duplicated.', 'mavicms-migrator' ),
					'stopped'       => __( 'Stopped. Press start to continue where you left off.', 'mavicms-migrator' ),
					'nothingToDo'   => __( 'There are no posts to migrate.', 'mavicms-migrator' ),
					'confirmReset'  => __( 'Forget which posts have been migrated? Content already sent to Mavi CMS is not removed, and migrating again would duplicate it.', 'mavicms-migrator' ),
					'unknownError'  => __( 'Something went wrong.', 'mavicms-migrator' ),
				),
			)
		);
	}
);

register_deactivation_hook(
	__FILE__,
	static function () {
		// The session is a live credential; the migration map is not, and
		// keeping it means reactivating does not re-send everything.
		delete_option( MaviCMS_Client::OPTION_SESSION );
	}
);
