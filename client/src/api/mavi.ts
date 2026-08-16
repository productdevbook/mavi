// Written from the description of the API. Do not edit by hand —
// `cargo test -p mavi-everything --test described` writes it, and
// fails when what was here is not what it wrote.

/** What every operation can answer with instead. */
export interface Refusal {
  /** Which refusal, exactly. Stable, and what a panel words in somebody's own language. */
  key: string;
  /** What the sentence needs: a name, a count, a limit. */
  named: Record<string, string>;
  /** The English, for anything with no wording of its own. */
  said: string;
}

/**
 * Whether this installation can answer at all. Nothing else — what asks this
 * is a container runtime rather than a person, and a detailed answer here
 * would be a description of somebody's installation handed to whoever asks.
 */
export interface Alive {
  /** Always true. An installation that would answer otherwise does not answer. */
  alive: boolean;
}

/** A JSON-RPC answer, or nothing at all where none was wanted. */
export interface AssistantAnswer {
  /** `2.0`. */
  jsonrpc?: string;
  /** Whatever was sent. */
  id?: unknown;
  /** What the method answered. */
  result?: unknown;
  /**
   * The protocol refusing, which is not a tool refusing — a tool that said
   * no is a `result` with `isError`.
   */
  error?: unknown;
}

/**
 * A JSON-RPC envelope. `initialize`, `tools/list` and `tools/call` are
 * answered; anything else is named back as not served.
 */
export interface AssistantAsked {
  /** `2.0`. */
  jsonrpc?: string;
  /** Which of them. */
  method: string;
  /** What the method takes. For `tools/call`: a `name` and its `arguments`. */
  params?: unknown;
  /** Left out means no answer is wanted, which is respected. */
  id?: unknown | null;
}

/**
 * What somebody is buying. Anybody may send this, which is why every rule is
 * on this side.
 */
export interface Basket {
  /** Where to reach them. */
  email: string;
  /**
   * What they are buying. Two lines for one thing is what pressing "add" twice
   * looks like from here, and is read as one line for the sum.
   */
  wanted: Wanted[];
  /** A coupon, if they typed one. */
  code?: string | null;
  /**
   * The caller's own, and the caller's to repeat: the same request twice is
   * one order. Held against the address it came with, so a guessed one does
   * not answer somebody else's order.
   */
  said_once: string;
}

/**
 * Where a card was dropped: which column, and between which two cards. Its
 * neighbours rather than a number, because what a person did is drop it
 * between two cards — the number is this software's business. Both
 * neighbours absent means an empty column.
 */
export interface Between {
  /** Which column it was dropped in. */
  stage: string;
  /** The card above it. */
  after?: string | null;
  /** The card below it. */
  before?: string | null;
}

/** Something a site works through in stages. */
export interface Board {
  /** Which one. */
  id: string;
  /** What it is called. */
  name: string;
  /** Its columns, left to right. */
  stages: Stage[];
  /** When it was made. */
  created_at: string;
}

/** Every board. A handful, with nothing to page through. */
export type BoardList = Board[];

/**
 * A whole site as a file. Its own shapes rather than the ones the API answers
 * with elsewhere, on purpose: what a listing answers may gain a field
 * tomorrow, and a file somebody wrote out last year still has to read.
 * Uploaded files, accounts, what people sent, what they bought, and how the
 * site looks are all deliberately not in it.
 */
export interface Bundle {
  /**
   * Which shape this file is. One from a later version is refused rather than
   * half read.
   */
  version: number;
  /** What it writes in. */
  languages?: BundledLanguage[];
  /** What it files things under. */
  terms?: BundledTerm[];
  /** Everything it wrote. */
  writings?: BundledWriting[];
}

/**
 * One language, in a file. Read back in it is never made the site's own —
 * which language a site writes in is a decision it has already made, and a
 * file does not get to change that from underneath whoever made it.
 */
export interface BundledLanguage {
  /** `en`, `tr`, `pt-BR`. */
  tag: string;
  /** What it is called, in itself. */
  name: string;
  /** What it was in the site this came from. Written out and not acted on. */
  is_the_sites_own: boolean;
}

/** One term, in a file. */
export interface BundledTerm {
  /**
   * Its own id **within this file** — what a writing here points at. Nothing
   * outside the file means anything by it, and reading it in gives it a new
   * one.
   */
  id: string;
  /** A category or a tag. */
  sort: string;
  /** Which language. */
  language: string;
  /** Where it answers. */
  slug: string;
  /** What it is called. */
  name: string;
  /** Which category it is under, by the ids in this file. */
  parent?: string | null;
}

/** One writing, in a file. */
export interface BundledWriting {
  /** Its own id within this file. */
  id: string;
  /** What the site decided it is. */
  kind: string;
  /** Which language. */
  language: string;
  /** Where it answers. */
  slug: string;
  /** What it is called. */
  title: string;
  /** A line about it. */
  excerpt?: string | null;
  /** What it says. */
  body?: string;
  /** Whatever the site kept beside it. */
  fields?: unknown;
  /** Whether it was out. */
  state: string;
  /** When it went out. */
  published_at?: string | null;
  /** What it is filed under, by the ids **in this file**. */
  terms?: string[];
}

/** One thing on a board. */
export interface Card {
  /** Which one. */
  id: string;
  /** Which board. */
  board_id: string;
  /** Which column it is in. */
  stage_id: string;
  /** What it says. */
  title: string;
  /** The rest of it. */
  detail: string | null;
  /** Whose it is. */
  owner: string | null;
  /**
   * Where it sits in its column. A fraction, so dropping one between two
   * others moves one row rather than every row below it.
   */
  place: number;
  /** When it was made. */
  created_at: string;
}

/**
 * What may be changed about one. Where it is is not among them: moving it is
 * `Between`.
 */
export interface CardChanges {
  /** What it says. */
  title?: string;
  /** The rest of it. */
  detail?: string;
  /** Whose it is. */
  owner?: string;
}

/** What is on one board. */
export interface CardPage {
  /** What is on this page. */
  items: Card[];
  /**
   * Where the next page starts. Absent means this is the last one — never a
   * cursor that answers an empty page.
   */
  next?: string;
}

/**
 * One set of changes to how a site looks. Everything written goes into one of
 * these — there is no way to write a file into what is published, which is
 * the whole shape of this said once.
 */
export interface Change {
  /** Which one. */
  id: string;
  /** What somebody called it. */
  name: string;
  /** Where it has got to. Only something built and looked at may be published. */
  at: "writing" | "to_look_at" | "broken" | "published";
  /**
   * Where to look at it, once it has been built. Under the build's own id,
   * which nothing links to.
   */
  look_at: string | null;
  /**
   * What the build said, where it did not build. Kept, because "it failed" is
   * not something anybody can act on.
   */
  went_wrong: string | null;
  /** When it was started. */
  created_at: string;
}

/** Every set of changes, newest first. */
export interface ChangePage {
  /** What is on this page. */
  items: Change[];
  /**
   * Where the next page starts. Absent means this is the last one — never a
   * cursor that answers an empty page.
   */
  next?: string;
}

/** One thing that is either well or not. */
export interface Check {
  /**
   * Which check. A key rather than a sentence, so a panel words it in
   * somebody's own language.
   */
  what: string;
  /** Whether it is. */
  well: boolean;
  /** What was found, where a number is what makes it worth reading. */
  detail: unknown;
}

/**
 * Choosing a password with a link somebody was sent. The link says what it was
 * minted for, and one minted to prove an address will not do this.
 */
export interface ChosenPassword {
  /** What was in the link. */
  token: string;
  /** What they chose. */
  password: string;
}

/**
 * What to write into a file. Never into what is published: this is always a
 * set of changes, which somebody looks at and publishes themselves.
 */
export interface Contents {
  /** What to put in it. */
  contents: string;
  /** Which set of changes to write it into. */
  change: string;
}

/** A code somebody types in. */
export interface Coupon {
  /**
   * Upper case, always. A code read off a poster and typed in lower case is
   * the same code, and the alternative is a discount that works for half the
   * people who try it.
   */
  code: string;
  /** Which of the two it takes off. */
  kind: "percent" | "amount";
  /** How many per cent. */
  percent: number | null;
  /** How much money. */
  amount: Money | null;
  /**
   * How many times it may be used at all. Null is as many as anybody likes,
   * which is a decision somebody made rather than a field left out.
   */
  at_most_uses: number | null;
  /** When it stops working. */
  expires_at: string | null;
}

/** Every code a shop has. A handful, with nothing to page through. */
export type CouponList = Coupon[];

/** Something a site teaches. */
export interface Course {
  /** Which one. */
  id: string;
  /** Where it answers. */
  slug: string;
  /** What it is called. */
  title: string;
  /** What it teaches. */
  about: string | null;
  /**
   * Where it has got to. A draft cannot be bought; a closed one keeps whoever
   * is already on it.
   */
  state: "draft" | "open" | "closed";
  /**
   * What it is made of. **Empty in a listing** — what a listing is for is
   * choosing a course, and carrying every lesson of every one of them is a
   * page that grows with the site rather than with the screen.
   */
  modules: Module[];
  /** When it was started. */
  created_at: string;
}

/**
 * What may be changed. Its address is not among them: it is what every link to
 * the course points at.
 */
export interface CourseChanges {
  /** What it is called. */
  title?: string;
  /** What it teaches. */
  about?: string;
  /** Where it has got to. */
  state?: "draft" | "open" | "closed";
}

/** What a site teaches. */
export interface CoursePage {
  /** What is on this page. */
  items: Course[];
  /**
   * Where the next page starts. Absent means this is the last one — never a
   * cursor that answers an empty page.
   */
  next?: string;
}

/**
 * Signing in. An address with no account and an address with the wrong
 * password are refused the same way, so this is not a way to ask which
 * addresses have accounts.
 */
export interface Credentials {
  /** Where they are reached. */
  email: string;
  /** What they typed. */
  password: string;
}

/** They are on it. */
export interface Enrolment {
  /** Which enrolment. */
  id: string;
  /** Which course. */
  course: string;
  /** Which student. */
  student: string;
}

/** How one page felt, by one measurement. */
export interface Felt {
  /** Which measurement. */
  kind: "lcp" | "inp" | "cls" | "ttfb";
  /** Which page. */
  path: string;
  /** The middle one — what the site is usually like. */
  middle: number;
  /**
   * What a twentieth of readers had worse than. The thing an average hides,
   * and the reason there is no average here.
   */
  bad_end: number;
  /** How many were measured. */
  how_many: number;
}

/** How the site felt, by page. */
export type FeltList = Felt[];

/** Something somebody uploaded. */
export interface File {
  /** Which one. */
  id: string;
  /**
   * What sort of thing it is. Decided by reading the bytes — what a file was
   * called is what somebody typed, and what they typed is not evidence.
   */
  kind: "image" | "video" | "audio" | "document";
  /**
   * What it is, exactly. From the bytes, and from a list — never from the
   * name.
   */
  mime: string;
  /**
   * What it was called when it arrived. Shown to people and used for nothing
   * else.
   */
  name: string;
  /**
   * Where it is kept. Opaque: whatever is holding it decides what this means,
   * and nothing reading it may take it apart.
   */
  kept_at: string;
  /** How big it is. */
  bytes: number;
  /** When it arrived. */
  created_at: string;
}

/** What a site has uploaded. */
export interface FilePage {
  /** What is on this page. */
  items: File[];
  /**
   * Where the next page starts. Absent means this is the last one — never a
   * cursor that answers an empty page.
   */
  next?: string;
}

/**
 * What one writing is filed under. Replaces whatever it was, so what is sent
 * is the whole of it rather than what to add.
 */
export interface Filing {
  /** Every term it is filed under now. */
  terms: string[];
}

/** Sending a form. Every rule is on this side, because anybody may. */
export interface Filled {
  /**
   * One value per field the form declared, by its key. A field it never asked
   * for is refused rather than kept.
   */
  answers: unknown;
}

/** What people sent. */
export interface FilledPage {
  /** What is on this page. */
  items: Sent[];
  /**
   * Where the next page starts. Absent means this is the last one — never a
   * cursor that answers an empty page.
   */
  next?: string;
}

/** Something a site does by itself when something happens. */
export interface Flow {
  /** Which one. */
  id: string;
  /** What it is called. */
  name: string;
  /** What sets it off. */
  trigger: "something_was_published" | "somebody_filled_in_a_form" | "an_order_was_paid_for" | "an_order_went_out" | "somebody_was_put_on_a_course" | "somebody_finished_a_course";
  /** Whether it runs at all. */
  on: boolean;
  /** What it does, in order. */
  steps: Step[];
  /** When it was arranged. */
  created_at: string;
}

/**
 * What may be changed. Not what sets it off: a flow arranged for one thing and
 * quietly moved to another is one nobody can reason about from its own runs.
 */
export interface FlowChanges {
  /** What it is called. */
  name?: string;
  /** Whether it runs at all. */
  on?: boolean;
  /**
   * The whole list, replaced. A flow's steps are one thing rather than a
   * collection to add to: what somebody is editing is the order and the
   * settings together.
   */
  steps?: NewStep[] | null;
}

/** What a site does by itself. */
export interface FlowPage {
  /** What is on this page. */
  items: Flow[];
  /**
   * Where the next page starts. Absent means this is the last one — never a
   * cursor that answers an empty page.
   */
  next?: string;
}

/**
 * The same thing as a page shows it. What it leaves out is the number: a shop
 * that answers "one left" to anybody who asks has published its stock list.
 * What a page needs is whether it can be bought.
 */
export interface ForSale {
  /** Where it answers. */
  slug: string;
  /** What it is called. */
  name: string;
  /** What it is. */
  about: string | null;
  /** What it costs. */
  price: Money;
  /** Whether somebody may buy it now. */
  can_be_bought: boolean;
}

/** What a shop is selling, as a page shows it. */
export interface ForSalePage {
  /** What is on this page. */
  items: ForSale[];
  /**
   * Where the next page starts. Absent means this is the last one — never a
   * cursor that answers an empty page.
   */
  next?: string;
}

/** What went, and what was emptied rather than taken away. */
export interface Forgotten {
  /** Accounts removed. */
  account: number;
  /** Places on a list removed. */
  on_lists: number;
  /** Enrolments removed. */
  learning: number;
  /**
   * Orders kept and emptied of the person. A bill that vanished is one nobody
   * can explain.
   */
  orders_emptied: number;
  /** Things sent through a form, removed. */
  sent_through_forms: number;
}

/** Something a site asks people, as whoever made it sees it. */
export interface Form {
  /** Which one. */
  id: string;
  /** Where it answers. */
  slug: string;
  /** What it is called. */
  name: string;
  /** What it asks for. */
  fields: FormField[];
  /**
   * Whether anybody may send it. A closed one answers the same way as one that
   * was never made, so the refusal is not a way to ask what this site has.
   */
  open: boolean;
  /** How long what people send is kept. */
  kept_days: number;
  /** When it was made. */
  created_at: string;
  /** When it last changed. */
  updated_at: string;
}

/**
 * What may be changed about one. Its address is not among them: it is what
 * every page carrying the form points at.
 */
export interface FormChanges {
  /** What it is called. */
  name?: string;
  /** What it asks for. Replaces whatever it asked for before. */
  fields?: FormField[] | null;
  /** Whether anybody may send it. */
  open?: boolean | null;
  /** How long what people send is kept. */
  kept_days?: number | null;
}

/** One thing a form asks for. */
export interface FormField {
  /**
   * What the answer comes back under. Also what a refusal names, so somebody
   * filling the form in is told which box was wrong.
   */
  key: string;
  /** What it says on the screen. */
  label: string;
  /** Whether it may be left empty. */
  required: boolean;
  /** Which box to draw. */
  kind: "text" | "long" | "email" | "number" | "choice" | "boolean";
  /**
   * What a `choice` may be. Empty for every other kind, and refused if it is
   * not.
   */
  options: string[];
}

/** What a site asks people. */
export interface FormPage {
  /** What is on this page. */
  items: Form[];
  /**
   * Where the next page starts. Absent means this is the last one — never a
   * cursor that answers an empty page.
   */
  next?: string;
}

/** What is wrong with this installation, where anything is. */
export interface Health {
  /** Whether every check was. */
  well: boolean;
  /** Each of them. */
  checks: Check[];
}

/**
 * What this site holds about one address, counted, across everything that
 * could hold it.
 */
export interface Held {
  /** An account here. */
  account: number;
  /** Places on a mailing list. */
  on_lists: number;
  /** As somebody learning here. */
  learning: number;
  /** Orders placed. */
  orders: number;
  /**
   * Things sent through a form whose answers name this address anywhere in
   * them.
   */
  sent_through_forms: number;
}

/**
 * Somebody to invite. The account exists immediately and has no password —
 * which is the difference between an invitation and a promise: whoever invited
 * them can see them in the list, and the link is the only way the account
 * becomes usable.
 */
export interface Invitation {
  /** Where to send the link. */
  email: string;
  /** What to call them. */
  name: string;
  /** Which role they hold. */
  role: string;
}

/** One language a site writes in. */
export interface Language {
  /**
   * A language tag: `en`, `tr`, `pt-BR`. The shape is checked rather than the
   * value looked up in a list — which languages exist is somebody else's
   * list and a copy of it here goes stale.
   */
  tag: string;
  /**
   * What it is called, in itself: `Türkçe` rather than `Turkish`. Whoever is
   * choosing it reads that one.
   */
  name: string;
  /** Whether this is the site's own. Exactly one is. */
  is_the_sites_own: boolean;
}

/** Every language a site writes in. A handful, with nothing to page through. */
export type LanguageList = Language[];

/** What one student is on. Theirs, and nobody else's. */
export type LearningList = Course[];

/** One lesson. */
export interface Lesson {
  /** Which one. */
  id: string;
  /** Which part it is in. */
  module_id: string;
  /** What it is called. */
  title: string;
  /** What it says. */
  body: string;
  /** Where it comes in the part. */
  place: number;
}

/**
 * What may be changed about one. Where it comes is not among them: moving it
 * is `TheOrder`, so that one lesson cannot be dragged somewhere without the
 * rest being told.
 */
export interface LessonChanges {
  /** What it is called. */
  title?: string;
  /** What it says. */
  body?: string;
}

/** One of a site's own letters, in one language. */
export interface Letter {
  /** Which letter this is. */
  kind: string;
  /** Which language it is in. */
  language: string;
  /** What the line at the top says. */
  subject: string;
  /** What it says. */
  body: string;
  /**
   * Whether a site wrote this. False means it is what this software says,
   * having been told nothing — which is why every kind is listed and not
   * only the ones somebody has edited.
   */
  theirs: boolean;
  /**
   * What this letter may name. Answered rather than written into a screen: a
   * panel that has to know the list is a panel that goes out of date.
   */
  names: string[];
}

/** Every letter a site sends, in one language. Every kind, always. */
export type LetterList = Letter[];

/** One of a site's mailing lists. */
export interface List {
  /** Which one. */
  id: string;
  /** What it is called. */
  name: string;
  /**
   * How many are on it **and may still be written to**. Not how many rows
   * there are: a list of nine hundred nobody may write to is a number that
   * tells whoever reads it the wrong thing.
   */
  reading: number;
  /** When it was made. */
  created_at: string;
}

/** Every list, and how many are on each. */
export type ListList = List[];

/** One part of a course. */
export interface Module {
  /** Which one. */
  id: string;
  /** What it is called. */
  title: string;
  /** Where it comes in the course. */
  place: number;
  /** What is in it. */
  lessons: Lesson[];
}

/**
 * An amount, in a currency. Never a number on its own: adding lira to euros is
 * not an arithmetic problem, and answering zero would be worse than refusing.
 */
export interface Money {
  /**
   * In the currency's smallest unit — kuruş, cents, pence. A whole number,
   * so nothing is ever half of one.
   */
  minor: number;
  /** Which currency, as ISO 4217. */
  currency: string;
}

/** One to make. */
export interface NewBoard {
  /** What it is called. */
  name: string;
  /**
   * The columns it starts with, left to right. At least one: a board with none
   * is a board nothing can be put on, so it is refused here rather than made
   * and then wondered about.
   */
  stages: string[];
}

/** One to put on a board. It goes at the bottom of its column. */
export interface NewCard {
  /** Which column. */
  stage: string;
  /** What it says. */
  title: string;
  /** The rest of it. */
  detail?: string | null;
  /** Whose it is. */
  owner?: string | null;
}

/**
 * A set of changes to start. It starts from what is published, so the files
 * that are live are copied in rather than left to be worked out later.
 */
export interface NewChange {
  /** What to call it. */
  name?: string;
}

/**
 * A code to make. Either a percentage or an amount with its currency — never
 * both, and never neither.
 */
export interface NewCoupon {
  /** What somebody types in. */
  code: string;
  /** How many per cent, one to a hundred. */
  percent?: number | null;
  /** How much, in the currency's smallest unit. */
  amount_minor?: number | null;
  /** Which currency the amount is in. */
  currency?: string | null;
  /** How many times it may be used at all. */
  at_most_uses?: number | null;
  /** When it stops working. */
  expires_at?: string | null;
}

/** One to start. */
export interface NewCourse {
  /** Where it should answer. */
  slug: string;
  /** What it is called. */
  title: string;
  /** What it teaches. */
  about?: string | null;
}

/** One to arrange. */
export interface NewFlow {
  /** What it is called. */
  name: string;
  /** What sets it off. */
  trigger: "something_was_published" | "somebody_filled_in_a_form" | "an_order_was_paid_for" | "an_order_went_out" | "somebody_was_put_on_a_course" | "somebody_finished_a_course";
  /** What it does, in order. */
  steps: NewStep[];
}

/** One to make. */
export interface NewForm {
  /** Where it should answer. */
  slug: string;
  /** What it is called. */
  name: string;
  /** What it asks for. */
  fields?: FormField[];
  /** How long what people send is kept. */
  kept_days?: number | null;
}

/** One to start writing in. */
export interface NewLanguage {
  /**
   * A language tag: `en`, `tr`, `pt-BR`. The shape is checked rather than the
   * value looked up in a list — which languages exist is somebody else's
   * list and a copy of it here goes stale.
   */
  tag: string;
  /** What it is called, in itself. */
  name: string;
}

/** A lesson to add. It goes on the end of its part. */
export interface NewLesson {
  /** What it is called. */
  title: string;
  /** What it says. */
  body: string;
}

/** One to make. */
export interface NewList {
  /** What to call it. */
  name: string;
}

/** A part to add. It goes on the end; moving it is `TheOrder`. */
export interface NewModule {
  /** What it is called. */
  title: string;
}

/** Something to put on the shelf. */
export interface NewProduct {
  /** Where it should answer. */
  slug: string;
  /** What it is called. */
  name: string;
  /** What it is. */
  about?: string | null;
  /** What it costs, in the currency's smallest unit. */
  price_minor: number;
  /** Which currency, as ISO 4217. */
  currency: string;
  /** How many there are. */
  on_the_shelf: number;
}

/** Somebody to put on a list. */
export interface NewReader {
  /** Where to reach them. */
  email: string;
  /** What to call them. */
  name?: string | null;
}

/**
 * One to make. Never the owner's: that one is made when the site is, exactly
 * once, and a second thing that can do everything is a second thing to have
 * taken.
 */
export interface NewRole {
  /** What it is called. */
  name: string;
  /**
   * What it holds, as `content:write` and the like. Each is checked against
   * the one list of capabilities, because a grant nobody spelled right is a
   * switch in a panel that looks on and does nothing.
   */
  grants?: string[];
}

/** One thing for a flow to do. */
export interface NewStep {
  /** Which of them. */
  does: "send_a_letter" | "call_an_address" | "wait" | "put_on_a_list";
  /**
   * What this step needs, by name. Which names depends on what it does, and
   * `flows.triggers` answers that list — an address to call, a letter to
   * send, how long to wait.
   */
  told?: unknown;
}

/** One to make. */
export interface NewTerm {
  /** Which of the two. */
  sort: "category" | "tag";
  /** Which language it is written in. */
  language: string;
  /** Where it should answer. */
  slug: string;
  /** What it is called. */
  name: string;
  /** Which category to put it under. Refused for a tag. */
  parent?: string | null;
}

/** Something to write. */
export interface NewWriting {
  /**
   * What a site decided this is. Lowercase, at most thirty-one characters.
   * `post` and `page` are what an installation starts with — a page is one
   * that is not in the feed — and a site may have as many others as it
   * likes.
   */
  kind: string;
  /** Which language it is written in. */
  language: string;
  /**
   * Where it should answer. Taken by the database rather than checked first,
   * so two people writing at one address is one of them told so.
   */
  slug: string;
  /** What it is called. Between one and two hundred characters. */
  title: string;
  /** A line about it. */
  excerpt?: string | null;
  /** What it says. */
  body?: string;
  /** Whatever this site decided to keep beside it. */
  fields?: unknown;
  /**
   * Left out means a draft. A date in the future is a thing that goes out on
   * it.
   */
  publish_at?: string | null;
}

/**
 * The same form as a page about to draw it sees it. Its own shape rather than
 * the whole one with fields left out — leaving things out is something
 * somebody has to keep doing, and what is missing here is everything about the
 * site rather than about the form.
 */
export interface OpenForm {
  /** Where it answers. */
  slug: string;
  /** What it is called. */
  name: string;
  /** What it asks for. */
  fields: FormField[];
}

/** Something somebody bought. */
export interface Order {
  /** Which one. */
  id: string;
  /** What somebody reads down a telephone. */
  number: number;
  /**
   * Where it has got to. Stock is held against one that is waiting, and put
   * back when it runs out.
   */
  state: "waiting" | "paid" | "sent" | "called_off" | "given_back";
  /** Where to reach whoever bought it. */
  email: string;
  /** What it came to. */
  total: Money;
  /** What is on it. */
  lines: OrderLine[];
  /** When it was placed. */
  created_at: string;
}

/**
 * One thing on an order, as it was when the order was placed. The name and the
 * price are copied rather than pointed at: what somebody bought does not
 * change because the shop renamed something afterwards.
 */
export interface OrderLine {
  /** What it was called. */
  name: string;
  /** What one cost. */
  each: Money;
  /** How many. */
  how_many: number;
}

/** What a shop has sold. */
export interface OrderPage {
  /** What is on this page. */
  items: Order[];
  /**
   * Where the next page starts. Absent means this is the last one — never a
   * cursor that answers an empty page.
   */
  next?: string;
}

/**
 * What this site has. Every number here is one somebody could have counted
 * from a listing they already hold — this is the same answer in one call
 * rather than eleven.
 */
export interface Overview {
  /** How many, including drafts. */
  writings: number;
  /** How many of them are out. */
  published: number;
  /** How many have been uploaded. */
  files: number;
  /** What they come to. */
  bytes: number;
  /** How many the site asks. */
  forms: number;
  /** What people sent that nobody has read. */
  unread: number;
  /**
   * How many may still be written to. Not how many rows there are: a list of
   * nine hundred nobody may write to is a number that tells whoever reads it
   * the wrong thing.
   */
  readers: number;
  /** How many are learning here. */
  students: number;
  /** How many have been placed. */
  orders: number;
  /** How many run by themselves. */
  flows_on: number;
  /**
   * Work the queue has stopped trying. A letter nobody received or a build
   * nobody got — on the first screen, because nothing else says so.
   */
  work_given_up_on: number;
}

/** Somebody with an account here. */
export interface Person {
  /** Which one. */
  id: string;
  /** Where they are reached. */
  email: string;
  /** What they are called. */
  name: string;
  /**
   * Which role they hold. What that role grants is the role's own answer —
   * repeating it on every person would be two answers to one question.
   */
  role: string;
  /** Whether the account may be used. */
  standing: string;
  /** When they proved the address is theirs. */
  proved_at: string | null;
  /** When they were invited. */
  created_at: string;
}

/** Who has an account here. */
export interface PersonPage {
  /** What is on this page. */
  items: Person[];
  /**
   * Where the next page starts. Absent means this is the last one — never a
   * cursor that answers an empty page.
   */
  next?: string;
}

/**
 * The order, and what it came to. Nothing else: whoever bought something is
 * not somebody this tells what else the shop has.
 */
export interface Placed {
  /** Which order. */
  id: string;
  /** What they read down a telephone. */
  number: number;
  /** What it came to. */
  total: Money;
}

/** The letter with the values in it, sent to nobody. */
export interface Pressed {
  /** The line at the top. */
  subject: string;
  /** What it says. */
  body: string;
}

/** Something a shop sells, as whoever runs it sees it. */
export interface Product {
  /** Which one. */
  id: string;
  /** Where it answers. */
  slug: string;
  /** What it is called. */
  name: string;
  /** What it is. */
  about: string | null;
  /** What it costs. */
  price: Money;
  /** How many there are. Not answered to anybody outside — see `ForSale`. */
  on_the_shelf: number;
  /** Whether it is being sold. */
  for_sale: boolean;
  /** When it was added. */
  created_at: string;
}

/**
 * What may be changed. Not its currency: an order already placed in one and a
 * price now in another is a shop that cannot add up its own orders.
 */
export interface ProductChanges {
  /** What it is called. */
  name?: string;
  /** What it is. */
  about?: string;
  /** What it costs, in the currency's smallest unit. */
  price_minor?: number;
  /** How many there are. */
  on_the_shelf?: number;
  /** Whether it is being sold. */
  for_sale?: boolean;
}

/** What a shop has. */
export interface ProductPage {
  /** What is on this page. */
  items: Product[];
  /**
   * Where the next page starts. Absent means this is the last one — never a
   * cursor that answers an empty page.
   */
  next?: string;
}

/** One lesson marked as done. */
export interface Progress {
  /** Which lesson. */
  lesson: string;
  /** When they finished it. */
  at: string;
}

/**
 * One file in a site's own project. Not a `File`, which is something somebody
 * uploaded: one is what a site is built from and the other is what a page puts
 * on the screen, and a client with one type for both has a type that is wrong
 * for one of them.
 */
export interface ProjectFile {
  /**
   * Where it is, under `src/` or `public/`. Whatever decides how a site is
   * built is refused on purpose: that is a way to run anything on the machine
   * that does the building.
   */
  path: string;
  /** What is in it. */
  contents: string;
  /** Whether this set of changes takes it away. */
  removed: boolean;
}

/** Every file in a project. The paths, without what is in them. */
export type ProjectFileList = ProjectFile[];

/**
 * Proving an address with a link sent to it. Touches nothing else — a link
 * that proves an address cannot set a password, which is the hole this shape
 * exists to keep closed.
 */
export interface Proof {
  /** What was in the link. */
  token: string;
}

/**
 * What anybody at all is told about this site. Its own shape rather than the
 * settings an editor reads, so that adding somewhere to reach a site's owner
 * does not put it on every page of the site.
 */
export interface PublicSite {
  /** What the site is called. */
  name: string;
  /** What it says about itself. */
  about: string | null;
  /** What it is written in. */
  languages: Language[];
}

/** One day of one page. */
export interface Read {
  /** Which day. */
  on_day: string;
  /** Which page. */
  path: string;
  /**
   * How many times it was read. Times, not people — telling those apart
   * means knowing where a request came from, and this software is not told
   * that.
   */
  views: number;
}

/** Every day of every page that was read, newest and busiest first. */
export type ReadList = Read[];

/** Somebody on a list. */
export interface Reader {
  /** Which one. */
  id: string;
  /** Where they are reached. */
  email: string;
  /** What they are called. */
  name: string | null;
  /** Whether they may still be written to. */
  standing: string;
  /** When they were added. */
  created_at: string;
}

/** Who is on one list. */
export interface ReaderPage {
  /** What is on this page. */
  items: Reader[];
  /**
   * Where the next page starts. Absent means this is the last one — never a
   * cursor that answers an empty page.
   */
  next?: string;
}

/**
 * The site, its owner, and the way in — in one answer, because the
 * alternative is telling somebody to sign in with the password they typed ten
 * seconds ago and hoping nothing went wrong in between.
 */
export interface Ready {
  /** The owner's account. */
  person: Person;
  /**
   * The token that signs them in. Sent as `Authorization: Bearer`. Handed over
   * once and kept nowhere: what this installation stores is a hash of it, so a
   * copy of the database is not a drawer of working keys.
   */
  token: string;
}

/**
 * One thing somebody did. Written in the same transaction as the change
 * itself, so a change that answered and left no receipt is not something that
 * can have happened.
 */
export interface Receipt {
  /** Which one. */
  id: string;
  /**
   * What sort of caller. `the_machine` is the site itself — a scheduled
   * publish, a sweep, a letter going out — because "nobody did this" is an
   * answer somebody will need one day.
   */
  who: "an_account" | "a_student" | "the_machine";
  /** Which one of them. */
  who_id: string | null;
  /**
   * The endpoint's own name — `writings.publish` — rather than a verb
   * chosen at the call site. Two names for one action is two answers to "what
   * happened to this".
   */
  did: string;
  /** What sort of thing it was about. */
  about: string;
  /** Which one. */
  about_id: string | null;
  /**
   * Whatever somebody reading this in a year needs in order to understand it
   * **without** the row it describes — which may since have been deleted,
   * and often has been.
   */
  what: unknown;
  /** Which request it came in on. */
  request: string;
  /** When. */
  created_at: string;
}

/** What has been done here. */
export interface ReceiptPage {
  /** What is on this page. */
  items: Receipt[];
  /**
   * Where the next page starts. Absent means this is the last one — never a
   * cursor that answers an empty page.
   */
  next?: string;
}

/**
 * It arrived. Nothing about the site comes back here — whoever filled the
 * form in is not somebody this tells what else exists.
 */
export interface Received {
  /** What arrived. */
  id: string;
}

/**
 * A name and a set of grants. An account holds exactly one, and that is the
 * whole of the permission system.
 */
export interface Role {
  /** Which one. */
  id: string;
  /** What it is called. */
  name: string;
  /**
   * What it holds, as `content:write` and the like. Each is checked against
   * the one list of capabilities, because a grant nobody spelled right is a
   * switch in a panel that looks on and does nothing.
   */
  grants: string[];
  /**
   * The one that can do everything, including the things nothing else may.
   * Exactly one exists; it is never made and never removed.
   */
  is_the_owner: boolean;
  /** When it was made. */
  created_at: string;
}

/**
 * What may be changed. The owner's may be renamed and what it holds may not be
 * touched — it holds everything by being what it is, and a set of grants
 * written onto it would be a second answer to what it can do, one that could
 * be made smaller.
 */
export interface RoleChanges {
  /** What it is called. */
  name?: string;
  /**
   * The whole set, replaced. What somebody is editing is which switches are
   * on, and sending only the ones they turned on would never turn one off.
   */
  grants?: string[] | null;
}

/**
 * Every role. A handful, with nothing to page through — a role picker with a
 * cursor in it is one somebody has to page through to find "Editor".
 */
export type RoleList = Role[];

/** One journey through a flow. */
export interface Run {
  /** Which one. */
  id: string;
  /** Which flow. */
  flow_id: string;
  /** Where it has got to. */
  state: string;
  /** What set it off, as it was at the moment it did. */
  about: unknown;
  /** Which step it is on. */
  at_step: number;
  /** What stopped it, where something did. */
  went_wrong: string | null;
  /** When it began. */
  started_at: string;
  /** When it ended. */
  finished_at: string | null;
}

/** What has run, newest first. */
export interface RunPage {
  /** What is on this page. */
  items: Run[];
  /**
   * Where the next page starts. Absent means this is the last one — never a
   * cursor that answers an empty page.
   */
  next?: string;
}

/** How many were marked as read. */
export interface Seen {
  /** How many had not been read and now have been. */
  seen: number;
}

/**
 * Something to send to everybody on a list who may still be written to. How
 * many went is what comes back.
 */
export interface Sending {
  /** The line at the top. */
  subject: string;
  /** What it says. */
  body: string;
  /** How many went. */
  letters?: number;
}

/**
 * One thing somebody sent. Where it came from is written down and is not here
 * — an address is about whoever filled the form in rather than about what
 * they said.
 */
export interface Sent {
  /** Which one. */
  id: string;
  /** Which form. */
  form_id: string;
  /** What they said, by the key each field declared. */
  answers: unknown;
  /** When somebody read it. */
  seen_at: string | null;
  /** When it arrived. */
  created_at: string;
}

/** Signed in. */
export interface Session {
  /** Who. */
  person: Person;
  /**
   * The token that signs them in. Sent as `Authorization: Bearer`. Handed over
   * once and kept nowhere: what this installation stores is a hash of it, so a
   * copy of the database is not a drawer of working keys.
   */
  token: string;
}

/** What a site says it is. */
export interface Settings {
  /** What the site is called. */
  name: string;
  /** What a page says about itself where it has nothing of its own to say. */
  about: string | null;
  /**
   * Which zone a site's own hours are in — when "tomorrow at nine" is, and
   * what day a report covers. Kept rather than guessed from the machine: a
   * machine is moved and a site is not.
   */
  time_zone: string;
}

/**
 * What may be changed. Whatever is sent is held against the whole of what the
 * settings would become, so a site cannot end up with a name and no time zone.
 */
export interface SettingsChanges {
  /** What the site is called. */
  name?: string;
  /** What a page says about itself. */
  about?: string;
  /** Which zone a site's own hours are in. */
  time_zone?: string;
}

/**
 * What making the site asks for. Answers once — an installation that has
 * been set up refuses this, which is the same thing a visitor learns by
 * looking at the front page.
 */
export interface Setup {
  /** What the site is called. */
  site: string;
  /** What the owner is called. */
  name: string;
  /** Where to reach them. */
  email: string;
  /** What they will sign in with. */
  password: string;
}

/** Which address this is about. */
export interface Somebody {
  /** Where they are reached. */
  email: string;
}

/** Somebody to invite to learn here. */
export interface SomebodyToAsk {
  /** Where to reach them. */
  email: string;
  /** What to call them. */
  name: string;
}

/**
 * Something invented, to try a flow against: whatever the thing that sets this
 * flow off would carry. Not described further, because what that is depends on
 * the trigger.
 */
export type SomethingMadeUp = Record<string, unknown>;

/**
 * A page was read. Sent by a reader's own browser — so what is here is what
 * a browser knows, and it is the whole of what is kept. Nothing about where it
 * arrived from is looked at, held, or written down.
 */
export interface SomethingRead {
  /**
   * Which page, as the reader's browser has it. Cut at five hundred characters
   * rather than refused.
   */
  path: string;
  /**
   * What was measured, where anything was. The web's own names: when the
   * biggest thing appeared, how long the page took to answer a tap, how much
   * moved, how long the server took.
   */
  felt?: "lcp" | "inp" | "cls" | "ttfb" | null;
  /**
   * Milliseconds, or a hundredth where the measurement is a ratio. Only read
   * where `felt` says what it is.
   */
  value?: number | null;
}

/** One column on a board. */
export interface Stage {
  /** Which one. */
  id: string;
  /** What it is called. */
  name: string;
  /** Where it comes, left to right. */
  place: number;
}

/** One thing a flow does, as it is read back. */
export interface Step {
  /** Which of them. */
  does: "send_a_letter" | "call_an_address" | "wait" | "put_on_a_list";
  /**
   * What this step needs, by name. Which names depends on what it does, and
   * `flows.triggers` answers that list — an address to call, a letter to
   * send, how long to wait.
   */
  told: unknown;
  /** Where it comes. */
  place: number;
}

/**
 * Somebody learning here. **Not a panel account** — what a student may reach
 * is their own lessons, and nothing about the site.
 */
export interface Student {
  /** Which one. */
  id: string;
  /** Where they are reached. */
  email: string;
  /** What they are called. */
  name: string;
  /** Whether they may still get in. */
  standing: string;
  /** When they were asked. */
  created_at: string;
}

/** Everybody learning here. */
export interface StudentPage {
  /** What is on this page. */
  items: Student[];
  /**
   * Where the next page starts. Absent means this is the last one — never a
   * cursor that answers an empty page.
   */
  next?: string;
}

/** Somewhere a site files things, or something it says they are about. */
export interface Term {
  /** Which one. */
  id: string;
  /**
   * A category is somewhere it lives and may be under another. A tag is
   * something it is about, and is flat.
   */
  sort: "category" | "tag";
  /** Which language it is written in. */
  language: string;
  /** Where it answers. */
  slug: string;
  /** What it is called. */
  name: string;
  /** Which category it is under. Always nothing for a tag. */
  parent: string | null;
  /** When it was made. */
  created_at: string;
  /** When it last changed. */
  updated_at: string;
}

/**
 * What may be changed about one. Its sort and its address are not among them:
 * those are what everything filed under it points at.
 */
export interface TermChanges {
  /** What it is called. */
  name?: string;
  /**
   * Which category to move it under. Null moves it out from under anything;
   * left out leaves it where it is.
   */
  parent?: string | null;
}

/**
 * All of them, with nothing to page through — what one writing is filed
 * under is a handful, not a listing.
 */
export type TermList = Term[];

/** What a site files things under. */
export interface TermPage {
  /** What is on this page. */
  items: Term[];
  /**
   * Where the next page starts. Absent means this is the last one — never a
   * cursor that answers an empty page.
   */
  next?: string;
}

/**
 * The same things in a new order. Refused if it is not exactly that: a list
 * with something missing is a lesson quietly dropped out of a course, and one
 * with something extra is a lesson from somebody else's course being pulled
 * into this one.
 */
export interface TheOrder {
  /** Every one of them, in the order they should come. */
  order: string[];
}

/** One thing somebody threw away. */
export interface Thrown {
  /** What sort of thing. The same word the address takes. */
  kind: "writings" | "files" | "terms" | "forms" | "products" | "courses" | "boards" | "cards" | "flows";
  /** Which one. */
  id: string;
  /**
   * Enough to know which one it is. A bin where nine rows say the same thing
   * is one nobody can restore from.
   */
  called: string;
  /** When it went in. */
  thrown_away_at: string;
}

/** What a site threw away, newest first, across every sort at once. */
export type ThrownList = Thrown[];

/**
 * What can set a flow off, and what a flow can do — with what each one has
 * to be told. Answered rather than written into a screen, so a step this build
 * does not have cannot be arranged.
 */
export interface TriggerList {
  /** Each with a `name`. */
  triggers: unknown;
  /** Each with a `name` and the `needs` it has to be told. */
  does: unknown;
}

/** What to put in a letter's names, to see what it would look like. */
export interface Values {
  /** One value per name the letter uses. */
  values: unknown;
}

/** One thing somebody is buying, and how many. */
export interface Wanted {
  /** Which one. */
  product: string;
  /** How many of it. */
  how_many: number;
}

/**
 * Every step, as it would run, and nothing sent. What this is for is seeing
 * what a flow would do before it does it to somebody.
 */
export type WhatItWouldDo = WouldDo[];

/**
 * What reading a file in did. Both halves, always — a number that only said
 * what was added would let somebody read a file into the wrong site, see
 * nothing added, and conclude the file was empty rather than that everything
 * in it was already there.
 */
export interface WhatWasRead {
  /** How many were added. */
  languages: number;
  /** How many were added. */
  terms: number;
  /** How many were added. */
  writings: number;
  /**
   * How many were already answering at the same address. Nothing is ever
   * overwritten, so reading a file in can only add.
   */
  left_alone: number;
}

/**
 * Where an order goes next. Which moves are allowed is the order's own rule
 * rather than the caller's — one that has gone out does not go back to
 * waiting.
 */
export interface WhereItGoes {
  /** Where to move it. */
  to: "paid" | "sent" | "called_off" | "given_back";
}

/** Which role to move somebody to. */
export interface WhichRole {
  /** Which one. */
  role: string;
}

/** Which student to put on this course. */
export interface WhoToPutOn {
  /** Which one. */
  student: string;
}

/**
 * What one of a site's letters should say. A name it has no value for is
 * refused rather than left in: an obvious hole says a great deal to whoever
 * wrote the letter and nothing at all to whoever receives one.
 */
export interface Wording {
  /** Which language. */
  language?: string;
  /** The line at the top. */
  subject: string;
  /** What it says. */
  body: string;
}

/** One step, as it would run. */
export interface WouldDo {
  /** Which of them. */
  does: "send_a_letter" | "call_an_address" | "wait" | "put_on_a_list";
  /** What it was told. */
  told: unknown;
  /**
   * What it would be working with, once the values from the thing that set it
   * off are put in.
   */
  about: unknown;
}

/** Something a site wrote. */
export interface Writing {
  /** Which one. */
  id: string;
  /**
   * What a site decided this is. Lowercase, at most thirty-one characters.
   * `post` and `page` are what an installation starts with — a page is one
   * that is not in the feed — and a site may have as many others as it
   * likes.
   */
  kind: string;
  /** Which language it is written in. */
  language: string;
  /** Where it answers. */
  slug: string;
  /** What it is called. */
  title: string;
  /** A line about it. */
  excerpt: string | null;
  /** What it says. */
  body: string;
  /** Whatever this site decided to keep beside it. */
  fields: unknown;
  /** Whether it is out. */
  state: "draft" | "published";
  /**
   * When it went out, or is going to. A date in the future is a thing that
   * goes out on it.
   */
  published_at: string | null;
  /** When it was written. */
  created_at: string;
  /** When it last changed. */
  updated_at: string;
}

/**
 * What may be changed about one. Only what is sent is changed — a change
 * that wrote every field would write back over whatever somebody else changed
 * a second ago.
 */
export interface WritingChanges {
  /** Where it answers. The old address keeps working. */
  slug?: string;
  /** What it is called. */
  title?: string;
  /** A line about it. */
  excerpt?: string;
  /** What it says. */
  body?: string;
  /** Whatever this site decided to keep beside it. */
  fields?: unknown;
  /**
   * Left out leaves it where it is. Null takes it back off the site; a date
   * sends it out then.
   */
  publish_at?: string | null;
}

/** What a site has written. */
export interface WritingPage {
  /** What is on this page. */
  items: Writing[];
  /**
   * Where the next page starts. Absent means this is the last one — never a
   * cursor that answers an empty page.
   */
  next?: string;
}


export interface Operation {
  method: "get" | "post" | "put" | "patch" | "delete";
  /** With its `{holes}` still in it. Filling them is the caller's. */
  path: string;
  /** The shape it takes, where it takes one. */
  takes: string | null;
  /** The shape it answers with, where it answers with one. */
  answers: string | null;
  /** What it answers when nothing went wrong. */
  status: number;
}

/** Every operation this installation describes. */
export const operations = {
  "about.forget": { method: "post", path: "/api/about/forget", takes: "Somebody", answers: "Forgotten", status: 200 },
  "about.gather": { method: "post", path: "/api/about", takes: "Somebody", answers: "Held", status: 200 },
  "addresses.prove": { method: "post", path: "/api/addresses", takes: "Proof", answers: null, status: 204 },
  "analytics.felt": { method: "get", path: "/api/analytics/felt", takes: null, answers: "FeltList", status: 200 },
  "analytics.read": { method: "get", path: "/api/analytics", takes: null, answers: "ReadList", status: 200 },
  "assistant.talk": { method: "post", path: "/api/assistant", takes: "AssistantAsked", answers: "AssistantAnswer", status: 200 },
  "audit.list": { method: "get", path: "/api/audit", takes: null, answers: "ReceiptPage", status: 200 },
  "audit.read": { method: "get", path: "/api/audit/{id}", takes: null, answers: "Receipt", status: 200 },
  "boards.list": { method: "get", path: "/api/boards", takes: null, answers: "BoardList", status: 200 },
  "boards.make": { method: "post", path: "/api/boards", takes: "NewBoard", answers: "Board", status: 201 },
  "boards.read": { method: "get", path: "/api/boards/{id}", takes: null, answers: "Board", status: 200 },
  "boards.remove": { method: "delete", path: "/api/boards/{id}", takes: null, answers: null, status: 204 },
  "cards.change": { method: "patch", path: "/api/cards/{id}", takes: "CardChanges", answers: "Card", status: 200 },
  "cards.list": { method: "get", path: "/api/boards/{id}/cards", takes: null, answers: "CardPage", status: 200 },
  "cards.make": { method: "post", path: "/api/boards/{id}/cards", takes: "NewCard", answers: "Card", status: 201 },
  "cards.move": { method: "put", path: "/api/cards/{id}/place", takes: "Between", answers: "Card", status: 200 },
  "cards.remove": { method: "delete", path: "/api/cards/{id}", takes: null, answers: null, status: 204 },
  "changes.build": { method: "post", path: "/api/design/changes/{id}/builds", takes: null, answers: null, status: 202 },
  "changes.list": { method: "get", path: "/api/design/changes", takes: null, answers: "ChangePage", status: 200 },
  "changes.publish": { method: "post", path: "/api/design/changes/{id}/published", takes: null, answers: null, status: 202 },
  "changes.read": { method: "get", path: "/api/design/changes/{id}", takes: null, answers: "Change", status: 200 },
  "changes.start": { method: "post", path: "/api/design/changes", takes: "NewChange", answers: "Change", status: 201 },
  "coupons.list": { method: "get", path: "/api/coupons", takes: null, answers: "CouponList", status: 200 },
  "coupons.make": { method: "post", path: "/api/coupons", takes: "NewCoupon", answers: "Coupon", status: 201 },
  "coupons.remove": { method: "delete", path: "/api/coupons/{code}", takes: null, answers: null, status: 204 },
  "courses.change": { method: "patch", path: "/api/courses/{id}", takes: "CourseChanges", answers: "Course", status: 200 },
  "courses.list": { method: "get", path: "/api/courses", takes: null, answers: "CoursePage", status: 200 },
  "courses.make": { method: "post", path: "/api/courses", takes: "NewCourse", answers: "Course", status: 201 },
  "courses.read": { method: "get", path: "/api/courses/{id}", takes: null, answers: "Course", status: 200 },
  "courses.reorder": { method: "put", path: "/api/courses/{id}/order", takes: "TheOrder", answers: "Course", status: 200 },
  "design.files": { method: "get", path: "/api/design/files", takes: null, answers: "ProjectFileList", status: 200 },
  "design.read": { method: "get", path: "/api/design/files/{path}", takes: null, answers: "ProjectFile", status: 200 },
  "design.write": { method: "put", path: "/api/design/files/{path}", takes: "Contents", answers: "ProjectFile", status: 200 },
  "enrolments.add": { method: "post", path: "/api/courses/{id}/students", takes: "WhoToPutOn", answers: "Enrolment", status: 201 },
  "enrolments.remove": { method: "delete", path: "/api/enrolments/{id}", takes: null, answers: null, status: 204 },
  "files.list": { method: "get", path: "/api/files", takes: null, answers: "FilePage", status: 200 },
  "files.read": { method: "get", path: "/api/files/{id}", takes: null, answers: "File", status: 200 },
  "files.remove": { method: "delete", path: "/api/files/{id}", takes: null, answers: null, status: 204 },
  "files.upload": { method: "post", path: "/api/files", takes: "TheBytes", answers: "File", status: 201 },
  "filled.forget": { method: "delete", path: "/api/filled/{id}", takes: null, answers: null, status: 204 },
  "flows.change": { method: "patch", path: "/api/flows/{id}", takes: "FlowChanges", answers: "Flow", status: 200 },
  "flows.list": { method: "get", path: "/api/flows", takes: null, answers: "FlowPage", status: 200 },
  "flows.make": { method: "post", path: "/api/flows", takes: "NewFlow", answers: "Flow", status: 201 },
  "flows.remove": { method: "delete", path: "/api/flows/{id}", takes: null, answers: null, status: 204 },
  "flows.triggers": { method: "get", path: "/api/flows/triggers", takes: null, answers: "TriggerList", status: 200 },
  "flows.try": { method: "post", path: "/api/flows/{id}/tries", takes: "SomethingMadeUp", answers: "WhatItWouldDo", status: 200 },
  "forms.change": { method: "patch", path: "/api/forms/{id}", takes: "FormChanges", answers: "Form", status: 200 },
  "forms.filled": { method: "get", path: "/api/forms/{id}/filled", takes: null, answers: "FilledPage", status: 200 },
  "forms.list": { method: "get", path: "/api/forms", takes: null, answers: "FormPage", status: 200 },
  "forms.make": { method: "post", path: "/api/forms", takes: "NewForm", answers: "Form", status: 201 },
  "forms.mark-seen": { method: "post", path: "/api/forms/{id}/seen", takes: null, answers: "Seen", status: 200 },
  "forms.read": { method: "get", path: "/api/forms/{id}", takes: null, answers: "Form", status: 200 },
  "forms.remove": { method: "delete", path: "/api/forms/{id}", takes: null, answers: null, status: 204 },
  "health.alive": { method: "get", path: "/api/alive", takes: null, answers: "Alive", status: 200 },
  "health.read": { method: "get", path: "/api/health", takes: null, answers: "Health", status: 200 },
  "languages.add": { method: "post", path: "/api/languages", takes: "NewLanguage", answers: "Language", status: 201 },
  "languages.forget": { method: "delete", path: "/api/languages/{tag}", takes: null, answers: null, status: 204 },
  "languages.list": { method: "get", path: "/api/languages", takes: null, answers: "LanguageList", status: 200 },
  "languages.make-own": { method: "put", path: "/api/languages/{tag}/own", takes: null, answers: "LanguageList", status: 200 },
  "learning.done": { method: "put", path: "/api/learning/lessons/{id}/done", takes: null, answers: "Progress", status: 200 },
  "learning.lesson": { method: "get", path: "/api/learning/lessons/{id}", takes: null, answers: "Lesson", status: 200 },
  "learning.mine": { method: "get", path: "/api/learning", takes: null, answers: "LearningList", status: 200 },
  "lessons.change": { method: "patch", path: "/api/lessons/{id}", takes: "LessonChanges", answers: "Lesson", status: 200 },
  "lessons.make": { method: "post", path: "/api/modules/{id}/lessons", takes: "NewLesson", answers: "Lesson", status: 201 },
  "lessons.remove": { method: "delete", path: "/api/lessons/{id}", takes: null, answers: null, status: 204 },
  "letters.forget": { method: "delete", path: "/api/mail/letters/{kind}", takes: null, answers: null, status: 204 },
  "letters.list": { method: "get", path: "/api/mail/letters", takes: null, answers: "LetterList", status: 200 },
  "letters.press": { method: "post", path: "/api/mail/letters/{kind}/pressed", takes: "Values", answers: "Pressed", status: 200 },
  "letters.write": { method: "put", path: "/api/mail/letters/{kind}", takes: "Wording", answers: "Letter", status: 200 },
  "lists.list": { method: "get", path: "/api/mail/lists", takes: null, answers: "ListList", status: 200 },
  "lists.make": { method: "post", path: "/api/mail/lists", takes: "NewList", answers: "List", status: 201 },
  "modules.make": { method: "post", path: "/api/courses/{id}/modules", takes: "NewModule", answers: "Module", status: 201 },
  "modules.remove": { method: "delete", path: "/api/modules/{id}", takes: null, answers: null, status: 204 },
  "modules.reorder": { method: "put", path: "/api/modules/{id}/order", takes: "TheOrder", answers: "Module", status: 200 },
  "open.fill-in": { method: "post", path: "/api/open/forms/{slug}", takes: "Filled", answers: "Received", status: 201 },
  "open.form": { method: "get", path: "/api/open/forms/{slug}", takes: null, answers: "OpenForm", status: 200 },
  "open.order": { method: "post", path: "/api/open/orders", takes: "Basket", answers: "Placed", status: 201 },
  "open.products": { method: "get", path: "/api/open/products", takes: null, answers: "ForSalePage", status: 200 },
  "open.read": { method: "post", path: "/api/open/read", takes: "SomethingRead", answers: null, status: 204 },
  "open.site": { method: "get", path: "/api/open/site", takes: null, answers: "PublicSite", status: 200 },
  "open.unsubscribe": { method: "post", path: "/api/open/mail/out/{token}", takes: null, answers: null, status: 204 },
  "orders.list": { method: "get", path: "/api/orders", takes: null, answers: "OrderPage", status: 200 },
  "orders.move": { method: "post", path: "/api/orders/{id}/moves", takes: "WhereItGoes", answers: "Order", status: 200 },
  "orders.read": { method: "get", path: "/api/orders/{id}", takes: null, answers: "Order", status: 200 },
  "passwords.choose": { method: "post", path: "/api/passwords", takes: "ChosenPassword", answers: null, status: 204 },
  "people.invite": { method: "post", path: "/api/people", takes: "Invitation", answers: "Person", status: 201 },
  "people.list": { method: "get", path: "/api/people", takes: null, answers: "PersonPage", status: 200 },
  "people.move": { method: "patch", path: "/api/people/{id}", takes: "WhichRole", answers: "Person", status: 200 },
  "people.remove": { method: "delete", path: "/api/people/{id}", takes: null, answers: null, status: 204 },
  "portable.read-in": { method: "post", path: "/api/portable", takes: "Bundle", answers: "WhatWasRead", status: 200 },
  "portable.take": { method: "get", path: "/api/portable", takes: null, answers: "Bundle", status: 200 },
  "products.change": { method: "patch", path: "/api/products/{id}", takes: "ProductChanges", answers: "Product", status: 200 },
  "products.list": { method: "get", path: "/api/products", takes: null, answers: "ProductPage", status: 200 },
  "products.make": { method: "post", path: "/api/products", takes: "NewProduct", answers: "Product", status: 201 },
  "products.remove": { method: "delete", path: "/api/products/{id}", takes: null, answers: null, status: 204 },
  "readers.add": { method: "post", path: "/api/mail/lists/{id}/readers", takes: "NewReader", answers: "Reader", status: 201 },
  "readers.forget": { method: "delete", path: "/api/mail/readers/{id}", takes: null, answers: null, status: 204 },
  "readers.list": { method: "get", path: "/api/mail/lists/{id}/readers", takes: null, answers: "ReaderPage", status: 200 },
  "roles.change": { method: "patch", path: "/api/roles/{id}", takes: "RoleChanges", answers: "Role", status: 200 },
  "roles.list": { method: "get", path: "/api/roles", takes: null, answers: "RoleList", status: 200 },
  "roles.make": { method: "post", path: "/api/roles", takes: "NewRole", answers: "Role", status: 201 },
  "roles.remove": { method: "delete", path: "/api/roles/{id}", takes: null, answers: null, status: 204 },
  "runs.list": { method: "get", path: "/api/flows/{id}/runs", takes: null, answers: "RunPage", status: 200 },
  "runs.read": { method: "get", path: "/api/runs/{id}", takes: null, answers: "Run", status: 200 },
  "sendings.send": { method: "post", path: "/api/mail/lists/{id}/sendings", takes: "Sending", answers: null, status: 202 },
  "sessions.begin": { method: "post", path: "/api/sessions", takes: "Credentials", answers: "Session", status: 201 },
  "sessions.end": { method: "delete", path: "/api/sessions", takes: null, answers: null, status: 204 },
  "settings.change": { method: "patch", path: "/api/settings", takes: "SettingsChanges", answers: "Settings", status: 200 },
  "settings.read": { method: "get", path: "/api/settings", takes: null, answers: "Settings", status: 200 },
  "setup.once": { method: "post", path: "/api/setup", takes: "Setup", answers: "Ready", status: 201 },
  "site.overview": { method: "get", path: "/api/overview", takes: null, answers: "Overview", status: 200 },
  "students.ask": { method: "post", path: "/api/students", takes: "SomebodyToAsk", answers: "Student", status: 201 },
  "students.list": { method: "get", path: "/api/students", takes: null, answers: "StudentPage", status: 200 },
  "terms.change": { method: "patch", path: "/api/terms/{id}", takes: "TermChanges", answers: "Term", status: 200 },
  "terms.list": { method: "get", path: "/api/terms", takes: null, answers: "TermPage", status: 200 },
  "terms.make": { method: "post", path: "/api/terms", takes: "NewTerm", answers: "Term", status: 201 },
  "terms.remove": { method: "delete", path: "/api/terms/{id}", takes: null, answers: null, status: 204 },
  "trash.for-good": { method: "delete", path: "/api/trash/{sort}/{id}", takes: null, answers: null, status: 204 },
  "trash.list": { method: "get", path: "/api/trash", takes: null, answers: "ThrownList", status: 200 },
  "trash.put-back": { method: "post", path: "/api/trash/{sort}/{id}", takes: null, answers: null, status: 204 },
  "writings.change": { method: "patch", path: "/api/writings/{id}", takes: "WritingChanges", answers: "Writing", status: 200 },
  "writings.file-under": { method: "put", path: "/api/writings/{id}/terms", takes: "Filing", answers: "TermList", status: 200 },
  "writings.list": { method: "get", path: "/api/writings", takes: null, answers: "WritingPage", status: 200 },
  "writings.read": { method: "get", path: "/api/writings/{id}", takes: null, answers: "Writing", status: 200 },
  "writings.throw-away": { method: "delete", path: "/api/writings/{id}", takes: null, answers: null, status: 204 },
  "writings.write": { method: "post", path: "/api/writings", takes: "NewWriting", answers: "Writing", status: 201 },
} as const;

export type Named = keyof typeof operations;


/**
 * What each call takes and gives. `never` is a call that takes nothing,
 * `void` is one that answers with nothing, and `Blob` is an upload — bytes,
 * whose kind gets decided by reading them rather than by anybody declaring
 * it.
 */
export interface Calls {
  "about.forget": { takes: Somebody; gives: Forgotten };
  "about.gather": { takes: Somebody; gives: Held };
  "addresses.prove": { takes: Proof; gives: void };
  "analytics.felt": { takes: never; gives: FeltList };
  "analytics.read": { takes: never; gives: ReadList };
  "assistant.talk": { takes: AssistantAsked; gives: AssistantAnswer };
  "audit.list": { takes: never; gives: ReceiptPage };
  "audit.read": { takes: never; gives: Receipt };
  "boards.list": { takes: never; gives: BoardList };
  "boards.make": { takes: NewBoard; gives: Board };
  "boards.read": { takes: never; gives: Board };
  "boards.remove": { takes: never; gives: void };
  "cards.change": { takes: CardChanges; gives: Card };
  "cards.list": { takes: never; gives: CardPage };
  "cards.make": { takes: NewCard; gives: Card };
  "cards.move": { takes: Between; gives: Card };
  "cards.remove": { takes: never; gives: void };
  "changes.build": { takes: never; gives: void };
  "changes.list": { takes: never; gives: ChangePage };
  "changes.publish": { takes: never; gives: void };
  "changes.read": { takes: never; gives: Change };
  "changes.start": { takes: NewChange; gives: Change };
  "coupons.list": { takes: never; gives: CouponList };
  "coupons.make": { takes: NewCoupon; gives: Coupon };
  "coupons.remove": { takes: never; gives: void };
  "courses.change": { takes: CourseChanges; gives: Course };
  "courses.list": { takes: never; gives: CoursePage };
  "courses.make": { takes: NewCourse; gives: Course };
  "courses.read": { takes: never; gives: Course };
  "courses.reorder": { takes: TheOrder; gives: Course };
  "design.files": { takes: never; gives: ProjectFileList };
  "design.read": { takes: never; gives: ProjectFile };
  "design.write": { takes: Contents; gives: ProjectFile };
  "enrolments.add": { takes: WhoToPutOn; gives: Enrolment };
  "enrolments.remove": { takes: never; gives: void };
  "files.list": { takes: never; gives: FilePage };
  "files.read": { takes: never; gives: File };
  "files.remove": { takes: never; gives: void };
  "files.upload": { takes: Blob; gives: File };
  "filled.forget": { takes: never; gives: void };
  "flows.change": { takes: FlowChanges; gives: Flow };
  "flows.list": { takes: never; gives: FlowPage };
  "flows.make": { takes: NewFlow; gives: Flow };
  "flows.remove": { takes: never; gives: void };
  "flows.triggers": { takes: never; gives: TriggerList };
  "flows.try": { takes: SomethingMadeUp; gives: WhatItWouldDo };
  "forms.change": { takes: FormChanges; gives: Form };
  "forms.filled": { takes: never; gives: FilledPage };
  "forms.list": { takes: never; gives: FormPage };
  "forms.make": { takes: NewForm; gives: Form };
  "forms.mark-seen": { takes: never; gives: Seen };
  "forms.read": { takes: never; gives: Form };
  "forms.remove": { takes: never; gives: void };
  "health.alive": { takes: never; gives: Alive };
  "health.read": { takes: never; gives: Health };
  "languages.add": { takes: NewLanguage; gives: Language };
  "languages.forget": { takes: never; gives: void };
  "languages.list": { takes: never; gives: LanguageList };
  "languages.make-own": { takes: never; gives: LanguageList };
  "learning.done": { takes: never; gives: Progress };
  "learning.lesson": { takes: never; gives: Lesson };
  "learning.mine": { takes: never; gives: LearningList };
  "lessons.change": { takes: LessonChanges; gives: Lesson };
  "lessons.make": { takes: NewLesson; gives: Lesson };
  "lessons.remove": { takes: never; gives: void };
  "letters.forget": { takes: never; gives: void };
  "letters.list": { takes: never; gives: LetterList };
  "letters.press": { takes: Values; gives: Pressed };
  "letters.write": { takes: Wording; gives: Letter };
  "lists.list": { takes: never; gives: ListList };
  "lists.make": { takes: NewList; gives: List };
  "modules.make": { takes: NewModule; gives: Module };
  "modules.remove": { takes: never; gives: void };
  "modules.reorder": { takes: TheOrder; gives: Module };
  "open.fill-in": { takes: Filled; gives: Received };
  "open.form": { takes: never; gives: OpenForm };
  "open.order": { takes: Basket; gives: Placed };
  "open.products": { takes: never; gives: ForSalePage };
  "open.read": { takes: SomethingRead; gives: void };
  "open.site": { takes: never; gives: PublicSite };
  "open.unsubscribe": { takes: never; gives: void };
  "orders.list": { takes: never; gives: OrderPage };
  "orders.move": { takes: WhereItGoes; gives: Order };
  "orders.read": { takes: never; gives: Order };
  "passwords.choose": { takes: ChosenPassword; gives: void };
  "people.invite": { takes: Invitation; gives: Person };
  "people.list": { takes: never; gives: PersonPage };
  "people.move": { takes: WhichRole; gives: Person };
  "people.remove": { takes: never; gives: void };
  "portable.read-in": { takes: Bundle; gives: WhatWasRead };
  "portable.take": { takes: never; gives: Bundle };
  "products.change": { takes: ProductChanges; gives: Product };
  "products.list": { takes: never; gives: ProductPage };
  "products.make": { takes: NewProduct; gives: Product };
  "products.remove": { takes: never; gives: void };
  "readers.add": { takes: NewReader; gives: Reader };
  "readers.forget": { takes: never; gives: void };
  "readers.list": { takes: never; gives: ReaderPage };
  "roles.change": { takes: RoleChanges; gives: Role };
  "roles.list": { takes: never; gives: RoleList };
  "roles.make": { takes: NewRole; gives: Role };
  "roles.remove": { takes: never; gives: void };
  "runs.list": { takes: never; gives: RunPage };
  "runs.read": { takes: never; gives: Run };
  "sendings.send": { takes: Sending; gives: void };
  "sessions.begin": { takes: Credentials; gives: Session };
  "sessions.end": { takes: never; gives: void };
  "settings.change": { takes: SettingsChanges; gives: Settings };
  "settings.read": { takes: never; gives: Settings };
  "setup.once": { takes: Setup; gives: Ready };
  "site.overview": { takes: never; gives: Overview };
  "students.ask": { takes: SomebodyToAsk; gives: Student };
  "students.list": { takes: never; gives: StudentPage };
  "terms.change": { takes: TermChanges; gives: Term };
  "terms.list": { takes: never; gives: TermPage };
  "terms.make": { takes: NewTerm; gives: Term };
  "terms.remove": { takes: never; gives: void };
  "trash.for-good": { takes: never; gives: void };
  "trash.list": { takes: never; gives: ThrownList };
  "trash.put-back": { takes: never; gives: void };
  "writings.change": { takes: WritingChanges; gives: Writing };
  "writings.file-under": { takes: Filing; gives: TermList };
  "writings.list": { takes: never; gives: WritingPage };
  "writings.read": { takes: never; gives: Writing };
  "writings.throw-away": { takes: never; gives: void };
  "writings.write": { takes: NewWriting; gives: Writing };
}
