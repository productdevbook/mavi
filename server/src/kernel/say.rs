//! What somebody is told, as a key rather than a sentence.
//!
//! A refusal answered in English is a refusal a panel cannot show in anybody
//! else's language, and one written in eleven places is one that says something
//! different in the twelfth. So what travels is a key, with the English beside
//! it in one file — for a log, for a client that has no catalogue of its own,
//! and so that reading the code still tells you what somebody sees.
//!
//! The panel's own translations live with the panel: shipping them twice is
//! shipping two of them, and the second one is always the stale one.

use std::fmt;

/// One thing somebody is told: a key, and whatever it has to name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Say {
    key: &'static str,
    named: Vec<(&'static str, String)>,
}

impl Say {
    #[must_use]
    pub const fn of(key: &'static str) -> Self {
        Self {
            key,
            named: Vec::new(),
        }
    }

    /// Something the sentence has to name — a count, a slug, a state. The panel
    /// puts these where its own wording wants them.
    #[must_use]
    pub fn naming(mut self, what: &'static str, value: impl fmt::Display) -> Self {
        self.named.push((what, value.to_string()));
        self
    }

    #[must_use]
    pub fn key(&self) -> &'static str {
        self.key
    }

    /// What was named, for a client putting the sentence together itself.
    #[must_use]
    pub fn named(&self) -> &[(&'static str, String)] {
        &self.named
    }
}

impl From<&'static str> for Say {
    fn from(key: &'static str) -> Self {
        Self::of(key)
    }
}

/// The English, with whatever it names put in. A key with no line here renders
/// as itself rather than as nothing, and a test refuses to let that happen.
impl fmt::Display for Say {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(text) = english(self.key) else {
            return f.write_str(self.key);
        };

        let mut out = text.to_owned();

        for (what, value) in &self.named {
            out = out.replace(&format!("{{{what}}}"), value);
        }

        f.write_str(&out)
    }
}

#[must_use]
pub fn english(key: &str) -> Option<&'static str> {
    ENGLISH
        .iter()
        .find(|(known, _)| *known == key)
        .map(|(_, text)| *text)
}

pub const ACCOUNT_NOT_BEEN_INVITED_SITE: &str = "account_not_been_invited_site";
pub const ADDRESS_NO_HOST: &str = "address_no_host";
pub const ADDRESS_NOT_ONE_MACHINE_WILL_SEND: &str = "address_not_one_machine_will_send";
pub const ADDRESS_NOT_RESOLVE: &str = "address_not_resolve";
pub const ADDRESS_ONLY_REACHED_OVER_HTTPS: &str = "address_only_reached_over_https";
pub const AMOUNT_CURRENCY_ARRIVE_TOGETHER_OR_NOT: &str = "amount_currency_arrive_together_or_not";
pub const ANSWER_TOO_LONG: &str = "answer_too_long";
pub const BUNDLE_WRITTEN_LANGUAGE_SITE_NOT: &str = "bundle_written_language_site_not";
pub const CAMPAIGN_NOT_ONE_CAN_STARTED: &str = "campaign_not_one_can_started";
pub const CODE_BEEN_USED: &str = "code_been_used";
pub const CODE_NOT_ONE_OURS: &str = "code_not_one_ours";
pub const COURSE_ALREADY_ANSWERS_NAME: &str = "course_already_answers_name";
pub const EXPORT_GIVEN_NOTHING_MAKES_ONE: &str = "export_given_nothing_makes_one";
pub const FILE_EMPTY: &str = "file_empty";
pub const FILE_TOO_LARGE: &str = "file_too_large";
pub const FORM_ALREADY_ANSWERS_ON_NAME: &str = "form_already_answers_on_name";
pub const IDEMPOTENCY_KEY_BETWEEN_EIGHT_TWO_HUNDRED: &str =
    "idempotency_key_between_eight_two_hundred";
pub const IMPORT_WANTS_BUNDLE: &str = "import_wants_bundle";
pub const KIND_FILE_NOT_TAKEN: &str = "kind_file_not_taken";
pub const LINK_BEEN_USED_OR_RUN_OUT: &str = "link_been_used_or_run_out";
pub const MAIL_SERVER_COULD_NOT_REACHED: &str = "mail_server_could_not_reached";
pub const MAIL_SERVER_DID_NOT_ANSWER: &str = "mail_server_did_not_answer";
pub const MESSAGE_CANNOT_SENT: &str = "message_cannot_sent";
pub const MOMENT_ALREADY_PASSED: &str = "moment_already_passed";
pub const NAME_ALREADY_USE: &str = "name_already_use";
pub const NAME_MAKES_NO_ADDRESS: &str = "name_makes_no_address";
pub const NOT_ADDRESS: &str = "not_address";
pub const NOT_ADDRESS_WRITE: &str = "not_address_write";
pub const NOT_BASE32: &str = "not_base32";
pub const NOT_EMAIL_ADDRESS: &str = "not_email_address";
pub const NOT_MAIL_SERVER_ADDRESS: &str = "not_mail_server_address";
pub const NOT_NAME_PROVIDER_CAN: &str = "not_name_provider_can";
pub const NOT_NUMBER_BUY: &str = "not_number_buy";
pub const NOT_PLACE: &str = "not_place";
pub const NOT_SIGN_WE_KNOW: &str = "not_sign_we_know";
pub const NOT_SIGNING_SECRET: &str = "not_signing_secret";
pub const NOT_SOMETHING_HAPPENS: &str = "not_something_happens";
pub const NOT_SOMETHING_PROVIDER_SAYS: &str = "not_something_provider_says";
pub const NOT_WHAT_ORDER_CAME: &str = "not_what_order_came";
pub const NOTE_BETWEEN_ONE_TEN_THOUSAND_CHARACTERS: &str =
    "note_between_one_ten_thousand_characters";
pub const ONLY_WHAT_UNDER_SRC_PUBLIC_CAN: &str = "only_what_under_src_public_can";
pub const ORDER_BEING_MADE: &str = "order_being_made";
pub const ORDER_BETWEEN_ONE_HUNDRED_LINES: &str = "order_between_one_hundred_lines";
pub const ORDER_LINES: &str = "order_lines";
pub const ORDER_ONE_CURRENCY: &str = "order_one_currency";
pub const BASKET_HAS_NOT_REACHED_WHAT_THAT_CODE_ASKS: &str =
    "basket_has_not_reached_what_that_code_asks";
pub const THAT_FORM_WANTS_THAT_FIELD: &str = "that_form_wants_that_field";
pub const THAT_FORM_HAS_NO_SUCH_FIELD: &str = "that_form_has_no_such_field";
pub const THAT_IS_NOT_WHAT_THAT_FIELD_HOLDS: &str = "that_is_not_what_that_field_holds";
pub const A_THING_DOES_NOT_SIT_UNDER_ITSELF: &str = "a_thing_does_not_sit_under_itself";
pub const THAT_ROLE_IS_THE_BUILD_S_OWN: &str = "that_role_is_the_build_s_own";
pub const SOMEBODY_IS_IN_THAT_ROLE: &str = "somebody_is_in_that_role";
pub const THAT_IS_NOT_A_KIND_OF_REPORT: &str = "that_is_not_a_kind_of_report";
pub const THAT_IS_NOT_A_STATE_A_COURSE_IS_IN: &str = "that_is_not_a_state_a_course_is_in";
pub const SOMETHING_IS_ALREADY_IN_THAT_PLACE: &str = "something_is_already_in_that_place";
pub const THAT_IS_NOT_A_LENGTH_OF_TIME: &str = "that_is_not_a_length_of_time";
pub const NOT_SOMETHING_TO_DO_TO_A_POST: &str = "not_something_to_do_to_a_post";
pub const TOO_MANY_AT_ONCE: &str = "too_many_at_once";
pub const PASSWORD_AT_LEAST_TWELVE_CHARACTERS: &str = "password_at_least_twelve_characters";
pub const PASSWORD_CANNOT_STORED: &str = "password_cannot_stored";
pub const PRICE_COUNT_NEVER_BELOW_ZERO: &str = "price_count_never_below_zero";
pub const PROVIDER_ANSWERED_NOTHING_USABLE: &str = "provider_answered_nothing_usable";
pub const PROVIDER_COULD_NOT_REACHED: &str = "provider_could_not_reached";
pub const PROVIDER_DID_NOT_SAY_ADDRESS: &str = "provider_did_not_say_address";
pub const PROVIDER_NOT_VERIFIED_ADDRESS: &str = "provider_not_verified_address";
pub const PROVIDER_ONLY_REACHED_OVER_HTTPS: &str = "provider_only_reached_over_https";
pub const PROVIDER_WOULD_NOT_EXCHANGE_CODE: &str = "provider_would_not_exchange_code";
pub const PROVIDER_WOULD_NOT_SAY_WHO: &str = "provider_would_not_say_who";
pub const PUBLISH_ALREADY_FINISHED: &str = "publish_already_finished";
pub const ROLE_S_KEY_LOWERCASE_LETTERS_UNDERSCORES: &str =
    "role_s_key_lowercase_letters_underscores";
pub const SCHEDULED_FOR_WHEN: &str = "scheduled_for_when";
pub const SETTINGS_NAMED_VALUES: &str = "settings_named_values";
pub const SIGN_NOT_ONE_SITE_WAITING_FOR: &str = "sign_not_one_site_waiting_for";
pub const SITE_ALREADY_BEING_PUBLISHED: &str = "site_already_being_published";
pub const SITE_CANNOT_TAKE_MONEY_YET: &str = "site_cannot_take_money_yet";
pub const SITE_NOT_WRITE_LANGUAGE: &str = "site_not_write_language";
pub const SLUG_LOWERCASE_LETTERS_DIGITS_SINGLE_HYPHENS: &str =
    "slug_lowercase_letters_digits_single_hyphens";
pub const SOMEBODY_ALREADY_USES_ADDRESS: &str = "somebody_already_uses_address";
pub const SOMEBODY_ELSE_CHANGES_WHAT: &str = "somebody_else_changes_what";
pub const SOMEBODY_ELSE_TAKES_ACCOUNT_AWAY: &str = "somebody_else_takes_account_away";
pub const SOMEBODY_MAY_ONLY_SENT_BACK_INTO: &str = "somebody_may_only_sent_back_into";
pub const SOMETHING_ALREADY_ANSWERS_ON_ADDRESS: &str = "something_already_answers_on_address";
pub const SOMETHING_ALREADY_SOLD_UNDER_NAME: &str = "something_already_sold_under_name";
pub const SOMETHING_ELSE_ANSWERS_NOW: &str = "something_else_answers_now";
pub const STEP_NO_LIST: &str = "step_no_list";
pub const STEP_NOBODY_ADD: &str = "step_nobody_add";
pub const STEP_NOBODY_WRITE: &str = "step_nobody_write";
pub const TAG_NOT_SIT_UNDER_ANOTHER: &str = "tag_not_sit_under_another";
pub const THERE_ALREADY_AUTHENTICATOR_ON_ACCOUNT: &str = "there_already_authenticator_on_account";
pub const THERE_ALREADY_ROLE_BY_NAME: &str = "there_already_role_by_name";
pub const THERE_NO_AUTHENTICATOR_WAITING_CONFIRMED: &str =
    "there_no_authenticator_waiting_confirmed";
pub const THERE_NO_SUCH_ROLE: &str = "there_no_such_role";
pub const THOSE_DIGITS_NOT_ONES_APP_SHOWING: &str = "those_digits_not_ones_app_showing";
pub const TITLE_BETWEEN_ONE_THREE_HUNDRED_CHARACTERS: &str =
    "title_between_one_three_hundred_characters";
pub const TITLE_MAKES_NO_ADDRESS: &str = "title_makes_no_address";
pub const TOO_MANY_ANSWERS: &str = "too_many_answers";
pub const TOOL: &str = "tool";
pub const TRANSFER_ALREADY_STOPPED: &str = "transfer_already_stopped";
pub const TRANSFER_EXPORT_OR_IMPORT: &str = "transfer_export_or_import";
pub const WHAT_BEING_SERVED_CHANGES_WHEN_PUBLISH: &str = "what_being_served_changes_when_publish";
pub const WHAT_BELONGED_NOT_THERE_ANY_MORE: &str = "what_belonged_not_there_any_more";
pub const BUNDLE_IN_A_LANGUAGE_THIS_SITE_DOES_NOT_USE: &str =
    "bundle_in_a_language_this_site_does_not_use";
pub const NO_SUCH_GRANT: &str = "no_such_grant";
pub const NOT_A_METHOD_HERE: &str = "not_a_method_here";
pub const NOT_THAT_MANY_LEFT: &str = "not_that_many_left";
pub const NOTHING_HERE_EMITS_THAT: &str = "nothing_here_emits_that";
pub const ONLY_SO_MANY_LEFT: &str = "only_so_many_left";
pub const ORDER_DOES_NOT_BECOME_THAT: &str = "order_does_not_become_that";
pub const PLUGIN_TAKES_NO_SUCH_SETTING: &str = "plugin_takes_no_such_setting";
pub const PLUGIN_WANTS_SETTING: &str = "plugin_wants_setting";
pub const POST_DOES_NOT_BECOME_THAT: &str = "post_does_not_become_that";
pub const ROLE_CARRIES_WHAT_YOU_DO_NOT: &str = "role_carries_what_you_do_not";
pub const SITE_DOES_NOT_BECOME_THAT: &str = "site_does_not_become_that";
pub const VERSION_READ_IS_NOT_VERSION_GIVEN: &str = "version_read_is_not_version_given";
pub const YOU_DO_NOT_HOLD_THAT_YOURSELF: &str = "you_do_not_hold_that_yourself";

pub const SITE_ALREADY_ANSWERS_TO_THAT_NAME: &str = "site_already_answers_to_that_name";
pub const ANOTHER_SITE_ANSWERS_ON_THAT_ADDRESS: &str = "another_site_answers_on_that_address";

pub const CHANGE_ASKED_FOR_FROM_SOMEWHERE_ELSE: &str = "change_asked_for_from_somewhere_else";

pub const A_KIND_OF_THING_IS_MADE_WITH_A_NAME: &str = "a_kind_of_thing_is_made_with_a_name";
pub const THERE_IS_ALREADY_A_KIND_OF_THING_BY_THAT_NAME: &str =
    "there_is_already_a_kind_of_thing_by_that_name";
pub const A_KIND_OF_THING_IS_NAMED_IN_LOWERCASE_AND_UNDERSCORES: &str =
    "a_kind_of_thing_is_named_in_lowercase_and_underscores";
pub const A_FIELD_IS_NAMED_IN_LOWERCASE_AND_UNDERSCORES: &str =
    "a_field_is_named_in_lowercase_and_underscores";
pub const A_KIND_OF_THING_SAYS_THAT_FIELD_TWICE: &str = "a_kind_of_thing_says_that_field_twice";
pub const A_CHOICE_FIELD_HAS_NOTHING_TO_CHOOSE_FROM: &str =
    "a_choice_field_has_nothing_to_choose_from";
pub const THAT_KIND_OF_THING_HAS_NO_SUCH_FIELD: &str = "that_kind_of_thing_has_no_such_field";
pub const THAT_KIND_OF_THING_WANTS_THAT_FIELD: &str = "that_kind_of_thing_wants_that_field";
pub const NOTHING_CAN_BE_ASKED_ABOUT_THAT_FIELD: &str = "nothing_can_be_asked_about_that_field";
pub const A_FIELD_IS_ASKED_ABOUT_ONE_WAY_AT_A_TIME: &str =
    "a_field_is_asked_about_one_way_at_a_time";
pub const ASKING_ABOUT_A_FIELD_MEANS_SAYING_WHICH_KIND: &str =
    "asking_about_a_field_means_saying_which_kind";

pub const FIELDS_BELONG_TO_A_KIND_OF_THING: &str = "fields_belong_to_a_kind_of_thing";

pub const THAT_FILE_IS_NOT_A_VIDEO: &str = "that_file_is_not_a_video";
pub const NOT_SOMETHING_A_TRANSCODER_SAYS: &str = "not_something_a_transcoder_says";

pub const THIS_MACHINE_IS_ALREADY_SET_UP: &str = "this_machine_is_already_set_up";

pub const THAT_SITE_HAS_NO_ROOM_LEFT: &str = "that_site_has_no_room_left";

pub const THAT_LETTER_CANNOT_NAME_THAT: &str = "that_letter_cannot_name_that";

pub const NOTHING_WAS_SAID: &str = "nothing_was_said";

pub const A_LINE_IS_A_PAYMENT_OR_AN_ADJUSTMENT: &str = "a_line_is_a_payment_or_an_adjustment";
pub const A_LINE_THAT_MOVES_NOTHING: &str = "a_line_that_moves_nothing";

pub const NOT_SOMETHING_A_POST_IS_MADE_OF: &str = "not_something_a_post_is_made_of";

pub const A_SITE_WRITES_IN_ONE_LANGUAGE_BY_DEFAULT: &str =
    "a_site_writes_in_one_language_by_default";
pub const SOMETHING_IS_WRITTEN_IN_THAT_LANGUAGE: &str = "something_is_written_in_that_language";
pub const THIS_SITE_ALREADY_WRITES_IN_THAT: &str = "this_site_already_writes_in_that";
pub const A_LANGUAGE_IS_TWO_LETTERS_AND_A_PLACE: &str = "a_language_is_two_letters_and_a_place";

pub const A_COUPON_TAKES_OFF_A_PERCENTAGE_OR_AN_AMOUNT: &str =
    "a_coupon_takes_off_a_percentage_or_an_amount";
pub const THAT_IS_NOT_A_DISCOUNT: &str = "that_is_not_a_discount";
pub const THERE_IS_ALREADY_A_COUPON_WITH_THAT_CODE: &str =
    "there_is_already_a_coupon_with_that_code";
pub const A_CODE_IS_BETWEEN_THREE_AND_FORTY_CHARACTERS: &str =
    "a_code_is_between_three_and_forty_characters";

pub const THAT_IS_THE_LAST_OWNER: &str = "that_is_the_last_owner";

/// Every key, and what it says in English. Adding a refusal is adding a line
/// here, which is the point: a sentence written at the place it is refused is a
/// sentence nobody can translate.
pub const ENGLISH: &[(&str, &str)] = &[
    (
        A_COUPON_TAKES_OFF_A_PERCENTAGE_OR_AN_AMOUNT,
        "a coupon takes off a percentage or an amount",
    ),
    (THAT_IS_NOT_A_DISCOUNT, "{value} is not a discount"),
    (
        THERE_IS_ALREADY_A_COUPON_WITH_THAT_CODE,
        "there is already a coupon with that code",
    ),
    (
        A_CODE_IS_BETWEEN_THREE_AND_FORTY_CHARACTERS,
        "a code is between three and forty characters",
    ),
    (
        THAT_IS_THE_LAST_OWNER,
        "that is the last owner, and there would be nobody left who could grant the role again",
    ),
    (
        A_SITE_WRITES_IN_ONE_LANGUAGE_BY_DEFAULT,
        "a site writes in one language by default, and this would leave it with none",
    ),
    (
        SOMETHING_IS_WRITTEN_IN_THAT_LANGUAGE,
        "there are {posts} things written in that language",
    ),
    (
        THIS_SITE_ALREADY_WRITES_IN_THAT,
        "this site already writes in that language",
    ),
    (
        A_LANGUAGE_IS_TWO_LETTERS_AND_A_PLACE,
        "a language is two letters, and a place after a hyphen where it has one",
    ),
    (
        NOT_SOMETHING_A_POST_IS_MADE_OF,
        "that is not something a post is made of",
    ),
    (
        A_LINE_IS_A_PAYMENT_OR_AN_ADJUSTMENT,
        "a line on an account is a payment or an adjustment",
    ),
    (A_LINE_THAT_MOVES_NOTHING, "that line moves nothing"),
    (NOTHING_WAS_SAID, "nothing was said"),
    (
        THAT_LETTER_CANNOT_NAME_THAT,
        "that letter has nothing to put where it says {name}",
    ),
    (
        THAT_SITE_HAS_NO_ROOM_LEFT,
        "this site is using {used} bytes of the {limit} it has room for",
    ),
    (
        THIS_MACHINE_IS_ALREADY_SET_UP,
        "this machine already has somebody running it",
    ),
    (THAT_FILE_IS_NOT_A_VIDEO, "that file is not a video"),
    (
        NOT_SOMETHING_A_TRANSCODER_SAYS,
        "that is not something a transcoder says",
    ),
    (
        FIELDS_BELONG_TO_A_KIND_OF_THING,
        "fields belong to a kind of thing, and this post is not one",
    ),
    (
        A_KIND_OF_THING_IS_MADE_WITH_A_NAME,
        "a kind of thing is made with a name",
    ),
    (
        THERE_IS_ALREADY_A_KIND_OF_THING_BY_THAT_NAME,
        "there is already a kind of thing by that name",
    ),
    (
        A_KIND_OF_THING_IS_NAMED_IN_LOWERCASE_AND_UNDERSCORES,
        "a kind of thing is named in lowercase letters and underscores",
    ),
    (
        A_FIELD_IS_NAMED_IN_LOWERCASE_AND_UNDERSCORES,
        "{field} is not a name a field can have",
    ),
    (
        A_KIND_OF_THING_SAYS_THAT_FIELD_TWICE,
        "{field} is declared twice",
    ),
    (
        A_CHOICE_FIELD_HAS_NOTHING_TO_CHOOSE_FROM,
        "{field} is a choice with nothing to choose from",
    ),
    (
        THAT_KIND_OF_THING_HAS_NO_SUCH_FIELD,
        "that kind of thing has no field called {field}",
    ),
    (
        THAT_IS_NOT_WHAT_THAT_FIELD_HOLDS,
        "that is not what {field} holds",
    ),
    (
        THAT_KIND_OF_THING_WANTS_THAT_FIELD,
        "that kind of thing wants {field}",
    ),
    (
        NOTHING_CAN_BE_ASKED_ABOUT_THAT_FIELD,
        "that kind of thing does not declare {field}, so nothing can be asked about it",
    ),
    (
        A_FIELD_IS_ASKED_ABOUT_ONE_WAY_AT_A_TIME,
        "a field is asked about one way at a time",
    ),
    (
        ASKING_ABOUT_A_FIELD_MEANS_SAYING_WHICH_KIND,
        "asking about a field means saying which kind of thing it belongs to",
    ),
    (
        CHANGE_ASKED_FOR_FROM_SOMEWHERE_ELSE,
        "a change asked for from somewhere else is not a change this site makes",
    ),
    (
        SITE_ALREADY_ANSWERS_TO_THAT_NAME,
        "a site already answers to that name",
    ),
    (
        ANOTHER_SITE_ANSWERS_ON_THAT_ADDRESS,
        "another site already answers on that address",
    ),
    (
        ACCOUNT_NOT_BEEN_INVITED_SITE,
        "that account has not been invited to this site",
    ),
    (ADDRESS_NO_HOST, "that address has no host"),
    (
        ADDRESS_NOT_ONE_MACHINE_WILL_SEND,
        "that address is not one this machine will send to",
    ),
    (ADDRESS_NOT_RESOLVE, "that address does not resolve"),
    (
        ADDRESS_ONLY_REACHED_OVER_HTTPS,
        "that address is only reached over https",
    ),
    (
        AMOUNT_CURRENCY_ARRIVE_TOGETHER_OR_NOT,
        "an amount and a currency arrive together or not at all",
    ),
    (ANSWER_TOO_LONG, "that answer is too long"),
    (
        BUNDLE_WRITTEN_LANGUAGE_SITE_NOT,
        "that bundle is written in a language this site does not have",
    ),
    (
        CAMPAIGN_NOT_ONE_CAN_STARTED,
        "that campaign is not one that can be started",
    ),
    (CODE_BEEN_USED, "that code has been used"),
    (CODE_NOT_ONE_OURS, "that code is not one of ours"),
    (
        COURSE_ALREADY_ANSWERS_NAME,
        "a course already answers to that name",
    ),
    (
        EXPORT_GIVEN_NOTHING_MAKES_ONE,
        "an export is given nothing; it makes one",
    ),
    (FILE_EMPTY, "that file is empty"),
    (FILE_TOO_LARGE, "that file is too large"),
    (
        FORM_ALREADY_ANSWERS_ON_NAME,
        "a form already answers on that name",
    ),
    (
        IDEMPOTENCY_KEY_BETWEEN_EIGHT_TWO_HUNDRED,
        "an idempotency key is between eight and two hundred characters",
    ),
    (IMPORT_WANTS_BUNDLE, "an import wants a bundle"),
    (KIND_FILE_NOT_TAKEN, "that kind of file is not taken here"),
    (
        LINK_BEEN_USED_OR_RUN_OUT,
        "that link has been used or has run out",
    ),
    (
        MAIL_SERVER_COULD_NOT_REACHED,
        "that mail server could not be reached",
    ),
    (
        MAIL_SERVER_DID_NOT_ANSWER,
        "that mail server did not answer",
    ),
    (MESSAGE_CANNOT_SENT, "that message cannot be sent"),
    (MOMENT_ALREADY_PASSED, "that moment has already passed"),
    (NAME_ALREADY_USE, "that name is already in use here"),
    (NAME_MAKES_NO_ADDRESS, "that name makes no address"),
    (NOT_ADDRESS, "that is not an address"),
    (NOT_ADDRESS_WRITE, "that is not an address to write to"),
    (NOT_BASE32, "that is not base32"),
    (NOT_EMAIL_ADDRESS, "that is not an email address"),
    (NOT_MAIL_SERVER_ADDRESS, "that is not a mail server address"),
    (
        NOT_NAME_PROVIDER_CAN,
        "that is not a name a provider can have",
    ),
    (NOT_NUMBER_BUY, "that is not a number to buy"),
    (NOT_PLACE, "that is not a place"),
    (NOT_SIGN_WE_KNOW, "that is not a sign-in we know"),
    (NOT_SIGNING_SECRET, "that is not a signing secret"),
    (NOT_SOMETHING_HAPPENS, "that is not something that happens"),
    (
        NOT_SOMETHING_PROVIDER_SAYS,
        "that is not something a provider says",
    ),
    (NOT_WHAT_ORDER_CAME, "that is not what the order came to"),
    (
        NOTE_BETWEEN_ONE_TEN_THOUSAND_CHARACTERS,
        "a note is between one and ten thousand characters",
    ),
    (
        ONLY_WHAT_UNDER_SRC_PUBLIC_CAN,
        "only what is under src/ and public/ can be written",
    ),
    (ORDER_BEING_MADE, "that order is being made"),
    (
        ORDER_BETWEEN_ONE_HUNDRED_LINES,
        "an order is between one and a hundred lines",
    ),
    (ORDER_LINES, "an order has lines"),
    (ORDER_ONE_CURRENCY, "an order is in one currency"),
    (
        BASKET_HAS_NOT_REACHED_WHAT_THAT_CODE_ASKS,
        "that code asks for a basket of at least {minimum}",
    ),
    (THAT_FORM_WANTS_THAT_FIELD, "that form wants {field}"),
    (
        THAT_FORM_HAS_NO_SUCH_FIELD,
        "that form has no field called {field}",
    ),
    (
        A_THING_DOES_NOT_SIT_UNDER_ITSELF,
        "a thing does not sit under itself",
    ),
    (
        THAT_ROLE_IS_THE_BUILD_S_OWN,
        "that role is this build's own, and is not a site's to change",
    ),
    (SOMEBODY_IS_IN_THAT_ROLE, "{people} people are in that role"),
    (THAT_IS_NOT_A_KIND_OF_REPORT, "that is not a kind of report"),
    (
        THAT_IS_NOT_A_STATE_A_COURSE_IS_IN,
        "that is not a state a course is in",
    ),
    (
        SOMETHING_IS_ALREADY_IN_THAT_PLACE,
        "something is already in that place",
    ),
    (THAT_IS_NOT_A_LENGTH_OF_TIME, "that is not a length of time"),
    (
        NOT_SOMETHING_TO_DO_TO_A_POST,
        "that is not something to do to a post",
    ),
    (TOO_MANY_AT_ONCE, "that is more than {most} at once"),
    (
        PASSWORD_AT_LEAST_TWELVE_CHARACTERS,
        "a password is at least twelve characters",
    ),
    (PASSWORD_CANNOT_STORED, "that password cannot be stored"),
    (
        PRICE_COUNT_NEVER_BELOW_ZERO,
        "a price and a count are never below zero",
    ),
    (
        PROVIDER_ANSWERED_NOTHING_USABLE,
        "that provider answered with nothing usable",
    ),
    (
        PROVIDER_COULD_NOT_REACHED,
        "that provider could not be reached",
    ),
    (
        PROVIDER_DID_NOT_SAY_ADDRESS,
        "that provider did not say an address",
    ),
    (
        PROVIDER_NOT_VERIFIED_ADDRESS,
        "that provider has not verified that address",
    ),
    (
        PROVIDER_ONLY_REACHED_OVER_HTTPS,
        "a provider is only reached over https",
    ),
    (
        PROVIDER_WOULD_NOT_EXCHANGE_CODE,
        "that provider would not exchange the code",
    ),
    (
        PROVIDER_WOULD_NOT_SAY_WHO,
        "that provider would not say who that is",
    ),
    (
        PUBLISH_ALREADY_FINISHED,
        "that publish has already finished",
    ),
    (
        ROLE_S_KEY_LOWERCASE_LETTERS_UNDERSCORES,
        "a role's key is lowercase letters and underscores",
    ),
    (SCHEDULED_FOR_WHEN, "scheduled for when?"),
    (SETTINGS_NAMED_VALUES, "settings are named values"),
    (
        SIGN_NOT_ONE_SITE_WAITING_FOR,
        "that sign-in is not one this site is waiting for",
    ),
    (
        SITE_ALREADY_BEING_PUBLISHED,
        "this site is already being published",
    ),
    (
        SITE_CANNOT_TAKE_MONEY_YET,
        "this site cannot take money yet",
    ),
    (
        SITE_NOT_WRITE_LANGUAGE,
        "this site does not write in that language",
    ),
    (
        SLUG_LOWERCASE_LETTERS_DIGITS_SINGLE_HYPHENS,
        "a slug is lowercase letters, digits and single hyphens",
    ),
    (
        SOMEBODY_ALREADY_USES_ADDRESS,
        "somebody here already uses that address",
    ),
    (
        SOMEBODY_ELSE_CHANGES_WHAT,
        "somebody else changes what you are",
    ),
    (
        SOMEBODY_ELSE_TAKES_ACCOUNT_AWAY,
        "somebody else takes an account away",
    ),
    (
        SOMEBODY_MAY_ONLY_SENT_BACK_INTO,
        "somebody may only be sent back into this site",
    ),
    (
        SOMETHING_ALREADY_ANSWERS_ON_ADDRESS,
        "something already answers on that address",
    ),
    (
        SOMETHING_ALREADY_SOLD_UNDER_NAME,
        "something is already sold under that name",
    ),
    (
        SOMETHING_ELSE_ANSWERS_NOW,
        "something else answers to that now",
    ),
    (STEP_NO_LIST, "that step has no list"),
    (STEP_NOBODY_ADD, "that step has nobody to add"),
    (STEP_NOBODY_WRITE, "that step has nobody to write to"),
    (
        TAG_NOT_SIT_UNDER_ANOTHER,
        "a tag does not sit under another",
    ),
    (
        THERE_ALREADY_AUTHENTICATOR_ON_ACCOUNT,
        "there is already an authenticator on this account",
    ),
    (
        THERE_ALREADY_ROLE_BY_NAME,
        "there is already a role by that name",
    ),
    (
        THERE_NO_AUTHENTICATOR_WAITING_CONFIRMED,
        "there is no authenticator waiting to be confirmed",
    ),
    (THERE_NO_SUCH_ROLE, "there is no such role"),
    (
        THOSE_DIGITS_NOT_ONES_APP_SHOWING,
        "those digits are not the ones the app is showing",
    ),
    (
        TITLE_BETWEEN_ONE_THREE_HUNDRED_CHARACTERS,
        "a title is between one and three hundred characters",
    ),
    (TITLE_MAKES_NO_ADDRESS, "that title makes no address"),
    (TOO_MANY_ANSWERS, "that is too many answers"),
    (TOOL, "which tool?"),
    (
        TRANSFER_ALREADY_STOPPED,
        "that transfer has already stopped",
    ),
    (
        TRANSFER_EXPORT_OR_IMPORT,
        "a transfer is an export or an import",
    ),
    (
        WHAT_BEING_SERVED_CHANGES_WHEN_PUBLISH,
        "what is being served changes when a publish says so",
    ),
    (
        WHAT_BELONGED_NOT_THERE_ANY_MORE,
        "what that belonged to is not there any more",
    ),
    (
        BUNDLE_IN_A_LANGUAGE_THIS_SITE_DOES_NOT_USE,
        "that bundle is written in {language}, which this site does not use",
    ),
    (NO_SUCH_GRANT, "there is no such grant: {grant}"),
    (NOT_A_METHOD_HERE, "{method} is not a method here"),
    (NOT_THAT_MANY_LEFT, "there are not that many {name} left"),
    (NOTHING_HERE_EMITS_THAT, "nothing here emits {event}"),
    (ONLY_SO_MANY_LEFT, "there are only {left} of {name} left"),
    (
        ORDER_DOES_NOT_BECOME_THAT,
        "a {from} order does not become {to}",
    ),
    (
        PLUGIN_TAKES_NO_SUCH_SETTING,
        "{plugin} does not take a setting called {setting}",
    ),
    (PLUGIN_WANTS_SETTING, "{plugin} wants {setting}"),
    (
        POST_DOES_NOT_BECOME_THAT,
        "a {from} post does not become {to}",
    ),
    (
        ROLE_CARRIES_WHAT_YOU_DO_NOT,
        "that role carries {beyond}, which you do not hold yourself",
    ),
    (
        SITE_DOES_NOT_BECOME_THAT,
        "a {from} site does not become {to}",
    ),
    (
        VERSION_READ_IS_NOT_VERSION_GIVEN,
        "this reads version {reads} and that is version {given}",
    ),
    (
        YOU_DO_NOT_HOLD_THAT_YOURSELF,
        "you do not hold {beyond} yourself",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_says_something() {
        for (key, text) in ENGLISH {
            assert!(!text.is_empty(), "{key} says nothing");
            assert!(
                key.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()),
                "{key} is not a key"
            );
        }
    }

    #[test]
    fn no_key_is_written_twice() {
        let mut keys: Vec<&str> = ENGLISH.iter().map(|(key, _)| *key).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();

        assert_eq!(before, keys.len(), "two lines say the same key");
    }

    #[test]
    fn what_a_sentence_names_is_put_in() {
        let said = Say::of(NOT_THAT_MANY_LEFT).naming("name", "a chair");

        assert_eq!(said.to_string(), "there are not that many a chair left");
        assert_eq!(said.key(), "not_that_many_left");
    }

    #[test]
    fn a_key_nothing_says_renders_as_itself_rather_than_as_nothing() {
        assert_eq!(
            Say::of("nothing_says_this").to_string(),
            "nothing_says_this"
        );
    }
}
