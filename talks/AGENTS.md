<!-- dek:begin (dek rewrites this block; write your own notes outside it) -->
# dek

A build system for talks. Write what you will say; dek builds, measures, and ships the rest.

## Principles

- `script.md` is the source of truth for order, script, and timing.
- Each slide is a `<section class="slide">` fragment.
- Conventions are enforced by lint. A deck is not done while `dek lint --visual` fails. Passing it means nothing measurable is wrong, not that the deck is good.

## Conventions

- One `##` heading is one slide. HTML lives in `slides/<id>.html`.
- Each deck owns its `theme.css`. Before writing a slide, run `dek theme` in the deck for the classes, tokens, and layouts it defines, and `dek theme <layout>` for a layout's markup.
- Shared look lives in `theme.css`. Decoration only one slide uses lives in `slides/<id>.css`, which is scoped to that slide.
- Use only classes defined in `theme.css` or in that slide's own `slides/<id>.css`.
- Color, type, space, radius, and motion in either stylesheet use token `var()` only. A value only one slide uses can be a token of its own on that slide's `.slide` rule in `slides/<id>.css`.
- Do not add `<style>`, `style=`, `<script>`, event handler attributes (`onclick=` and the like), or `javascript:` URLs inside slide HTML.
- Motion CSS cannot express lives in `slides/<id>.ts`: `export default { motion: { <step>: ms }, draw(slide, { index, step, t }) {} } satisfies DekSlide`. `DekSlide` is global, from `.dek/slide.d.ts`; do not import it. Key the slide's arrival, before its first beat, as `"0"`: every `data-step` element is hidden there, so draw what the slide shows before anything happens. Draw from `t` alone and set everything you touch on every call, with no timers and no imports, so video and screenshots can seek it. In `draw`, find elements by data-* attributes from the slide it is given, never by class or through `document`: the built deck holds every slide.
- Keep the deck self-contained: no remote URLs and no paths outside the deck.
- Every slide carries its place in `script.md` as `--dek-slide-number` and `--dek-slide-count`. Print a folio from them in `theme.css`, never by hand: `.slide { counter-reset: folio var(--dek-slide-number) }`, then `content: counter(folio)`. It follows the script as slides move.

## Checking a slide

- `dek check <slug> --shot` lints one slide and screenshots it at its last beat.
- `dek shot --sheet` tiles every slide on one image: read it to judge the deck's balance in one look, then open a slide's own shot for detail.
- Mark decoration `aria-hidden="true"`: a glow that bleeds off the slide, or a sample of text the talk shows as unreadable. Lint measures neither overflow nor contrast on it, and screen readers skip it, so never mark text the audience should read.
- When a hint sends a fix to `theme.css`, make it there, not in `slides/<id>.css`: the theme alone draws it that way, so other slides share the problem, and one change fixes them all.
- One shot shows no motion. `dek shot <slug> --motion` lays the slide's beats out as rows, each held at moments through everything it moves and ending as the shot does. `dek shot <a> --to <b> --at 0.5` freezes the view transition between any two slides.

## After a rehearsal

- `dek marks` lists the beats the speaker marked while rehearsing aloud (`m` in the presenter view): the words they stumbled over are `was`, at `line` in `script.md`. Rewrite those beats to be easier to say, and keep each beat's heading so its mark follows it; `status` turns `edited` once the words changed.
- Leave `dek marks clear` to the speaker: only saying the new words aloud tells whether they work.

## Before you report a deck as done

- `dek lint --visual` passes.
- You have read `dek shot --sheet` and judged the deck's balance.
- Say what you could not judge, such as the argument and the timing, and leave it to the author.

## References

Other people's decks, pinned in `dek.toml` `[refs]`, to read as models. They are read-only: read a slide with `dek show <ref> <slug>`, then write your own in this deck's theme.

- `hajimism/deck/care-swing`: ケアと成熟は、デザインできるか
- `hajimism/deck/context-design`: すべてはコンテキストデザインである
- `hajimism/deck/five-compounds`: しごとに埋め込む5つの複利
- `hajimism/dek/lightning`: 三分で、発表をひとつ作る
- `hajimism/dek/why-dek`: 台本から、発表まで
- `hajimism/dek/with-agents`: スライドを、エージェントに任せる

For commands, run `dek help --agent`.
<!-- dek:end -->
