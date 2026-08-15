=== Migrate to Mavi CMS ===
Contributors: productdevbook
Tags: migration, export, cms, headless
Requires at least: 5.9
Tested up to: 6.7
Requires PHP: 7.4
Stable tag: 0.2.0
License: GPLv2 or later
License URI: https://www.gnu.org/licenses/gpl-2.0.html

Sends your WordPress posts, categories, tags and images to a Mavi CMS site.

== Description ==

Mavi CMS is a self-hosted CMS with a Rust backend. This plugin moves an existing
WordPress site into it over the API: posts with their original dates and
permalinks, the category tree, tags, featured images and the images inside post
content.

Nothing in WordPress is changed or deleted. The migration is resumable — a post
that has already been sent is skipped, so you can stop and continue at any time.

Multilingual sites using Polylang or WPML keep their languages, and translations
of the same post are linked together on the Mavi CMS side.

== Installation ==

1. Upload the plugin and activate it.
2. Go to Tools → Migrate to Mavi CMS.
3. Enter the address of your Mavi CMS site and sign in.
4. Press "Start migration".

Your Mavi CMS site fetches images directly from this site, so this site has to
be reachable from it while the migration runs.

== Frequently Asked Questions ==

= Is my password stored? =

No. It is used once to sign in, and only the resulting session is kept.

= What happens if I run it twice? =

Posts that have already been migrated are skipped.

= Are images copied or hotlinked? =

Copied. Mavi CMS downloads each image and rewrites the post content to point at
its own copy. An image it cannot fetch keeps its original address, and the
failure is shown in the log.

== Changelog ==

= 0.1.7 =
* Copy images in post content even when WordPress has no matchable media
  record for them, which is most of what a page builder produces. They were
  being skipped in silence, leaving posts loading their pictures from the site
  they had just left.
* Link a translation as it is migrated, rather than only in a pass at the end,
  so an interrupted run leaves what it did send correctly linked.

= 0.1.6 =
* Adopt posts that are already on the destination instead of failing on them.
  After "Forget history" every post that had been migrated came back as an
  unexplained conflict, which stalled the whole run.
* Report the language the destination actually stored, rather than the one
  that was asked for.

= 0.1.5 =
* Move the content language next to the migrate button, where it belongs — it
  is a migration setting, not part of connecting.
* Name the language each post was migrated into, in the log.

= 0.1.4 =
* Migrate posts in every language. Polylang narrows admin queries to the
  language selected in the admin bar, so posts in the other languages were
  never even listed for migration, let alone sent. WPML does the same and is
  handled too.
* Show, before anything is sent, how many posts are in each language and where
  each group will end up — including posts no multilingual plugin has tagged,
  which go to the chosen fallback language.
* Create a language on the destination when a post first needs it, instead of
  only when connecting.

= 0.1.3 =
* Report posts that were skipped because they had already been migrated. A
  second run used to log nothing at all, so a run where everything was skipped
  looked identical to one that had done nothing.

= 0.1.2 =
* Save the content language when it is changed. It was only stored while
  connecting, so changing it meant signing in again — and the password field
  is cleared after connecting.

= 0.1.1 =
* Rewrite every image URL in post content, not only the one in `src`. `srcset`
  candidates and links to the full-size file used to be left pointing at the
  old site.
* Drop `srcset` and `sizes`, since one file now replaces the whole set of
  generated sizes.

= 0.1.0 =
* First release.
