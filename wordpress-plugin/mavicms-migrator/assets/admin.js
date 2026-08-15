/* global MaviCMSMigrator */
(function () {
	'use strict';

	var settings = window.MaviCMSMigrator || {};
	var queue = [];
	var index = 0;
	var running = false;
	var tally = { migrated: 0, warned: 0, failed: 0, skipped: 0 };

	function element(id) {
		return document.getElementById(id);
	}

	function log(message, kind) {
		var line = document.createElement('p');
		line.className = 'mavicms-log-line mavicms-log-' + (kind || 'info');
		line.textContent = message;
		var box = element('mavicms-log');
		box.appendChild(line);
		box.scrollTop = box.scrollHeight;
	}

	function post(action, data) {
		var body = new FormData();
		body.append('action', action);
		body.append('nonce', settings.nonce);
		Object.keys(data || {}).forEach(function (key) {
			body.append(key, data[key]);
		});

		return fetch(settings.ajaxUrl, {
			method: 'POST',
			credentials: 'same-origin',
			body: body,
		})
			.then(function (response) {
				return response.json();
			})
			.then(function (payload) {
				if (!payload || !payload.success) {
					throw new Error(
						(payload && payload.data && payload.data.message) || settings.i18n.unknownError
					);
				}
				return payload.data;
			});
	}

	function connect() {
		var status = element('mavicms-connection-status');
		status.textContent = settings.i18n.connecting;

		post('mavicms_connect', {
			site: element('mavicms-site').value.trim().replace(/\/+$/, ''),
			username: element('mavicms-username').value,
			password: element('mavicms-password').value,
			locale: element('mavicms-locale').value,
		})
			.then(function (data) {
				status.textContent = data.message;
				element('mavicms-password').value = '';
				element('mavicms-start').disabled = false;
				element('mavicms-locale').disabled = false;

				var select = element('mavicms-locale');
				select.innerHTML = '';
				(data.languages || []).forEach(function (language) {
					var option = document.createElement('option');
					option.value = language.code;
					option.textContent = language.native_name + ' (' + language.code + ')';
					option.selected = language.code === data.locale;
					select.appendChild(option);
				});
			})
			.catch(function (error) {
				status.textContent = '';
				log(error.message, 'error');
			});
	}

	function updateProgress() {
		var total = queue.length;
		var bar = element('mavicms-bar');
		bar.max = total;
		bar.value = index;
		element('mavicms-progress-text').textContent = settings.i18n.progress
			.replace('%1$d', index)
			.replace('%2$d', total);
	}

	function step() {
		if (!running) {
			return;
		}
		if (index >= queue.length) {
			finish();
			return;
		}

		var postId = queue[index];
		post('mavicms_migrate_post', { post_id: postId })
			.then(function (data) {
				tally[data.status] = (tally[data.status] || 0) + 1;
				if ('failed' === data.status) {
					log('#' + postId + ' — ' + data.message, 'error');
				} else if ('warned' === data.status) {
					log(data.message, 'warn');
				} else if ('migrated' === data.status) {
					log(data.message, 'ok');
				}
				// A skipped post logs nothing of its own — one line each would
				// bury the real news on a second run — but it is counted and
				// reported in the summary below.
			})
			.catch(function (error) {
				tally.failed += 1;
				log('#' + postId + ' — ' + error.message, 'error');
			})
			.then(function () {
				index += 1;
				updateProgress();
				// One post at a time on purpose: the destination fetches every
				// image itself, and firing the whole archive at once is how a
				// small server gets knocked over.
				step();
			});
	}

	function finish() {
		log(
			settings.i18n.summary
				.replace('%1$d', tally.migrated + tally.warned)
				.replace('%2$d', tally.skipped)
				.replace('%3$d', tally.failed),
			tally.failed ? 'warn' : 'ok'
		);
		if (tally.skipped && !tally.migrated && !tally.warned) {
			log(settings.i18n.allSkipped, 'warn');
		}

		log(settings.i18n.linking);
		post('mavicms_link_translations')
			.then(function (data) {
				if (data.linked) {
					log(settings.i18n.linked.replace('%d', data.linked), 'ok');
				}
				log(settings.i18n.done, 'ok');
			})
			.catch(function (error) {
				log(error.message, 'error');
			})
			.then(function () {
				running = false;
				element('mavicms-start').style.display = '';
				element('mavicms-stop').style.display = 'none';
			});
	}

	function start() {
		queue = settings.postIds || [];
		index = 0;
		running = true;
		tally = { migrated: 0, warned: 0, failed: 0, skipped: 0 };

		if (!queue.length) {
			log(settings.i18n.nothingToDo);
			running = false;
			return;
		}

		element('mavicms-start').style.display = 'none';
		element('mavicms-stop').style.display = '';
		element('mavicms-progress').style.display = '';
		// The count rendered with the page is a snapshot from before this run;
		// leaving it up would contradict the live one right below it.
		element('mavicms-stored-progress').style.display = 'none';
		updateProgress();
		step();
	}

	function stop() {
		running = false;
		element('mavicms-start').style.display = '';
		element('mavicms-stop').style.display = 'none';
		log(settings.i18n.stopped);
	}

	function saveLocale() {
		var status = element('mavicms-connection-status');
		post('mavicms_set_locale', { locale: element('mavicms-locale').value })
			.then(function () {
				status.textContent = settings.i18n.languageSaved;
				// The breakdown table names this language as the destination for
				// untagged posts, and would otherwise still show the old one.
				var target = element('mavicms-fallback-target');
				if (target) {
					target.textContent = element('mavicms-locale').value;
				}
			})
			.catch(function (error) {
				log(error.message, 'error');
			});
	}

	function reset() {
		if (!window.confirm(settings.i18n.confirmReset)) {
			return;
		}
		post('mavicms_reset')
			.then(function (data) {
				log(data.message, 'ok');
			})
			.catch(function (error) {
				log(error.message, 'error');
			});
	}

	document.addEventListener('DOMContentLoaded', function () {
		element('mavicms-connect').addEventListener('click', connect);
		element('mavicms-start').addEventListener('click', start);
		element('mavicms-stop').addEventListener('click', stop);
		element('mavicms-reset').addEventListener('click', reset);
		element('mavicms-locale').addEventListener('change', saveLocale);
	});
})();
