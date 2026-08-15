<?php
/**
 * The migration screen.
 *
 * @package MaviCMS_Migrator
 *
 * @var MaviCMS_Client   $client    Configured API client.
 * @var MaviCMS_Migrator $migrator  Migrator instance.
 * @var bool             $connected Whether a session is stored.
 * @var array            $progress  Counts for the progress display.
 * @var array            $languages Content languages of the destination.
 * @var string           $locale    Currently chosen default language.
 * @var array            $breakdown Post counts per source language.
 */

defined( 'ABSPATH' ) || exit;
?>
<div class="wrap mavicms-migrator">
	<h1><?php esc_html_e( 'Migrate to Mavi CMS', 'mavicms-migrator' ); ?></h1>

	<p class="description">
		<?php esc_html_e( 'Sends your posts, categories, tags and images to a Mavi CMS site. Nothing here is changed or deleted.', 'mavicms-migrator' ); ?>
	</p>

	<h2 class="title"><?php esc_html_e( '1. Connect', 'mavicms-migrator' ); ?></h2>
	<table class="form-table" role="presentation">
		<tr>
			<th scope="row"><label for="mavicms-site"><?php esc_html_e( 'Mavi CMS address', 'mavicms-migrator' ); ?></label></th>
			<td>
				<input type="url" id="mavicms-site" class="regular-text" placeholder="https://example.com"
					value="<?php echo esc_attr( $client->site_url() ); ?>" />
				<p class="description"><?php esc_html_e( 'The address of your Mavi CMS site, without a trailing slash.', 'mavicms-migrator' ); ?></p>
			</td>
		</tr>
		<tr>
			<th scope="row"><label for="mavicms-username"><?php esc_html_e( 'Username', 'mavicms-migrator' ); ?></label></th>
			<td><input type="text" id="mavicms-username" class="regular-text" autocomplete="off" /></td>
		</tr>
		<tr>
			<th scope="row"><label for="mavicms-password"><?php esc_html_e( 'Password', 'mavicms-migrator' ); ?></label></th>
			<td>
				<input type="password" id="mavicms-password" class="regular-text" autocomplete="off" />
				<p class="description"><?php esc_html_e( 'Used once to sign in. Only the resulting session is stored, never the password.', 'mavicms-migrator' ); ?></p>
			</td>
		</tr>
	</table>
	<p>
		<button type="button" class="button" id="mavicms-connect"><?php esc_html_e( 'Connect', 'mavicms-migrator' ); ?></button>
		<span id="mavicms-connection-status" class="description">
			<?php echo $connected ? esc_html__( 'A session is stored.', 'mavicms-migrator' ) : ''; ?>
		</span>
	</p>

	<h2 class="title"><?php esc_html_e( '2. Migrate', 'mavicms-migrator' ); ?></h2>
	<?php if ( count( $breakdown ) > 1 || isset( $breakdown[''] ) ) : ?>
		<table class="widefat striped" style="max-width:32em;margin-bottom:1em">
			<thead>
				<tr>
					<th><?php esc_html_e( 'Language in WordPress', 'mavicms-migrator' ); ?></th>
					<th><?php esc_html_e( 'Posts', 'mavicms-migrator' ); ?></th>
					<th><?php esc_html_e( 'Goes to', 'mavicms-migrator' ); ?></th>
				</tr>
			</thead>
			<tbody>
				<?php foreach ( $breakdown as $code => $count ) : ?>
					<tr>
						<td>
							<?php
							echo '' === $code
								? '<em>' . esc_html__( 'not set', 'mavicms-migrator' ) . '</em>'
								: esc_html( $code );
							?>
						</td>
						<td><?php echo (int) $count; ?></td>
						<td <?php echo '' === $code ? 'id="mavicms-fallback-target"' : ''; ?>><?php echo esc_html( '' === $code ? $locale : $code ); ?></td>
					</tr>
				<?php endforeach; ?>
			</tbody>
		</table>
		<?php if ( isset( $breakdown[''] ) ) : ?>
			<p class="description" style="max-width:40em">
				<?php esc_html_e( 'Posts with no language are ones no multilingual plugin has tagged. They all go to the language chosen below, whatever language they are actually written in.', 'mavicms-migrator' ); ?>
			</p>
		<?php endif; ?>
	<?php endif; ?>

	<p id="mavicms-stored-progress">
		<?php
		printf(
			/* translators: 1: number of migrated posts, 2: total number of posts. */
			esc_html__( '%1$d of %2$d posts have been migrated.', 'mavicms-migrator' ),
			(int) $progress['done'],
			(int) $progress['total']
		);
		?>
	</p>
	<p>
		<label for="mavicms-locale"><?php esc_html_e( 'Content language', 'mavicms-migrator' ); ?></label>
		<select id="mavicms-locale" <?php disabled( ! $connected ); ?> style="margin-right:.75em">
			<?php foreach ( $languages as $language ) : ?>
				<option value="<?php echo esc_attr( $language['code'] ); ?>" <?php selected( $language['code'], $locale ); ?>>
					<?php echo esc_html( $language['native_name'] . ' (' . $language['code'] . ')' ); ?>
				</option>
			<?php endforeach; ?>
		</select>
		<button type="button" class="button button-primary" id="mavicms-start" <?php disabled( ! $connected ); ?>>
			<?php esc_html_e( 'Start migration', 'mavicms-migrator' ); ?>
		</button>
		<button type="button" class="button" id="mavicms-stop" style="display:none">
			<?php esc_html_e( 'Stop', 'mavicms-migrator' ); ?>
		</button>
		<button type="button" class="button button-link-delete" id="mavicms-reset">
			<?php esc_html_e( 'Forget history', 'mavicms-migrator' ); ?>
		</button>
	</p>
	<p class="description" style="max-width:40em">
		<?php esc_html_e( 'The language above is only used for posts that have none of their own; posts tagged by Polylang or WPML keep theirs. A post that has already been sent is skipped, so you can stop and continue at any time.', 'mavicms-migrator' ); ?>
	</p>

	<div id="mavicms-progress" style="display:none">
		<progress id="mavicms-bar" value="0" max="100" style="width:100%;height:20px"></progress>
		<p id="mavicms-progress-text" class="description"></p>
	</div>

	<h2 class="title"><?php esc_html_e( 'Log', 'mavicms-migrator' ); ?></h2>
	<div id="mavicms-log" class="mavicms-log" aria-live="polite"></div>
</div>
