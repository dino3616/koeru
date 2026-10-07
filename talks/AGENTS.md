<!-- dek:begin (dek rewrites this block; write your own notes outside it) -->
# dek

A build system for talks. Write what you will say; dek builds, measures, and ships the rest.

## Principles

- `script.md` is the source of truth for order, script, and timing.
- Each slide is a `<section class="slide">` fragment.
- Conventions are enforced by lint. A deck is not done while `dekc lint --visual` fails. Passing it means nothing measurable is wrong, not that the deck is good.

## Conventions

- One `##` heading is one slide. HTML lives in `slides/<id>.html`.
- Each deck owns its `theme.css`, `decks/<deck>/theme.css`: lint, the build, and `dekc theme` read that one, and it is the `theme.css` this file and every hint mean. The `theme.css` at the project root is only the template `dekc new` copies into a new deck; editing it changes no deck that exists.
- Before writing a slide, run `dekc theme` in the deck for the classes, tokens, and layouts it defines, and `dekc theme <layout>` for a layout's markup.
- Shared look lives in the deck's `theme.css`. Decoration only one slide uses lives in `slides/<id>.css`, which is scoped to that slide.
- Use only classes defined in `theme.css` or in that slide's own `slides/<id>.css`. The theme holds at most `max_classes` in the frontmatter of `script.md`, 40 by default (`DEK013`); classes in `slides/<id>.css` do not count, so keep a class only one slide uses there.
- `is-current` and `is-shown` are states the player sets: `is-current` on the slide on screen, `is-shown` on each `data-step` element once its beat plays. `dekc theme` lists them apart, as state classes. Select them in CSS, never write them in markup (`DEK010`).
- A rule in `slides/<id>.css` weighs as if it were written at the end of `theme.css`: it beats the theme's `.slide .x`, but not a more specific rule such as a layout's `.slide[data-layout="split"] .x` or the beat state `.slide.is-current [data-step]`. To override one of those, write the same selector.
- In either stylesheet, colors, font families, easings, and lengths and times in an absolute, viewport, container, or root unit (`px`, `rem`, `vw`, `ms`, and the like) come from a token's `var()` (`DEK014`). Keywords such as `bold` or `thin`, unitless numbers, `%`, and units of the element's own font (`em`, `ch`, `lh`) are not raw values and pass. A value only one slide uses can be a token of its own on that slide's `.slide` rule in `slides/<id>.css`.
- Do not add `<style>`, `style=`, `<script>`, event handler attributes (`onclick=` and the like), or `javascript:` URLs inside slide HTML.
- Motion CSS cannot express lives in `slides/<id>.ts`: `export default { motion: { <step>: ms }, draw(slide, { index, step, t }) {} } satisfies DekSlide`. `DekSlide` is global, from `.dek/slide.d.ts`; do not import it. Key the slide's arrival, before its first beat, as `"0"`: every `data-step` element is hidden there, so draw what the slide shows before anything happens. Draw from `t` alone and set everything you touch on every call, with no timers and no imports, so video and screenshots can seek it. In `draw`, find elements by data-* attributes from the slide it is given, never by class or through `document`: the built deck holds every slide.
- Every picture says what it shows, or that it is decoration (`DEK034`). An `<img>` takes `alt`, `alt=""` for decoration. An `<svg>` with no text of its own takes `role="img"` and an `aria-label`, or `aria-hidden="true"` when it is decoration; one with `<text>` is read as it is. An element with `role="img"` takes an `aria-label`.
- Keep the deck self-contained: no remote URLs and no paths outside the deck.
- Every slide carries its place in `script.md` as `--dek-slide-number` and `--dek-slide-count`. Print a folio from them in `theme.css`, never by hand: `.slide { counter-reset: folio var(--dek-slide-number) }`, then `content: counter(folio)`. It follows the script as slides move.

## Checking a slide

- `dekc check <slug> --shot` lints one slide and screenshots it at its last beat.
- In `dekc check <slug> --json`, `fill` says how much of the frame the slide fills at its last beat, and where: `coverage`, the `box` it lies in, and `rows` and `columns`, the share of each tenth. It counts what the audience reads or looks at: text, pictures, and painted boxes that hold nothing, as a chart's bars. A card counts by what it holds, so one with its words at the top leaves its lower half empty; decoration under `aria-hidden` counts too. Read it before you open the shot to see whether a slide is sparse or leaves a band empty. Judge by `rows`, `columns`, and `box`, where an empty band is a run of zeros: `coverage` alone says little, since a chapter door in large type and a dense slide can share it. Whether a band left empty is right for the slide is yours to judge.
- A shot's path names what it drew, and an edit to the slide or the theme replaces the file. Never reuse a shot's path from before an edit: run `dekc shot <slug>` again, which shoots only what changed, and read the path it prints.
- `dekc shot --sheet` tiles every slide on one image: read it to judge the deck's balance in one look, then open a slide's own shot for detail.
- Mark decoration `aria-hidden="true"`: a glow that bleeds off the slide, or a sample of text the talk shows as unreadable. Lint measures neither overflow nor contrast on it, and screen readers skip it, so never mark text the audience should read: `DEK029` warns of text under it.
- When a hint sends a fix to the deck's `theme.css`, make it there, not in `slides/<id>.css`: the theme alone draws it that way, so other slides share the problem, and one change fixes them all.
- One shot shows no motion. `dekc shot <slug> --motion` lays the slide's beats out as rows, each held at moments through everything it moves and ending as the shot does. Its `--json` puts the sheets under `sheets` and each beat's frames under `motion`; `shots` is empty. `dekc shot <a> --to <b> --at 0.5` freezes the view transition between any two slides.

## After a rehearsal

- `dekc marks` lists the beats the speaker marked while rehearsing aloud (`m` in the presenter view): the words they stumbled over are `was`, at `line` in `script.md`. Rewrite those beats to be easier to say, and keep each beat's heading so its mark follows it; `status` turns `edited` once the words changed.
- Leave `dekc marks clear` to the speaker: only saying the new words aloud tells whether they work.

## When the human points at an element

- `dekc annotations` lists the notes the human wrote on elements of the slides (`a` on the dev server's page): each element at `path`, `line`, and `column` in the file as it is now, with its `name`, the note's `text`, and the `shot` that shows the slide at the beat the human saw. Read them yourself when the human says to deal with their notes. Fix each, then list them again: `status` turns `edited` once the slide's own files changed and `gone` once none of the note's elements is left. `edited` does not mean a note is dealt with: compare the `shot` with the note.
- Leave `dekc annotations clear` to the human: they wrote the notes and judge the result.

## Before you report a deck as done

- `dekc lint --visual` passes.
- You have read `dekc shot --sheet` and judged the deck's balance.
- Say what you could not judge, such as the argument and the timing, and leave it to the author.

## References

Other people's decks, pinned in `dek.toml` `[refs]`, to read as models. They are read-only: read a slide with `dekc show <ref> <slug>`, then write your own in this deck's theme.

- `hajimism/deck/care-swing`: ケアと成熟は、デザインできるか
- `hajimism/deck/context-design`: すべてはコンテキストデザインである
- `hajimism/deck/five-compounds`: しごとに埋め込む5つの複利
- `hajimism/deck/playful-work`: 力の湧く仕事とは？
- `hajimism/deck/trait-repetition`: 仕事のきほんシリーズ②：個性の本質は反復である
- `hajimism/deck/value-change`: 仕事のきほんシリーズ①：価値の本質は変化である
- `hajimism/dek/lightning`: 三分で、発表をひとつ作る
- `hajimism/dek/why-dek`: 台本から、発表まで
- `hajimism/dek/with-agents`: スライドを、エージェントに任せる

For commands, run `dekc help --agent`.
<!-- dek:end -->
