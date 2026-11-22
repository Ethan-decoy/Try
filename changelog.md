# Changelog

Notable changes follow [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Compatibility for stable releases starts at 1.0.0. See the [release policy](docs/release_policy.md).

## [Unreleased]

These changes are planned for **1.0.0**, the first stable release. Back up your data directory before upgrading from a 0.x preview. Try reuses data it can read, with no guaranteed migration from previews.

### Added

- Choose Text or Formula separately for prompts, answers and choices in the question editor.
- Add a Show card hints preference under Settings → Card. Hiding the answer and dismissal instructions preserves the card layout, keyboard shortcuts, and corrections.
- On Windows, resize practice cards from a minimum of 320 × 288 logical pixels and restore automatic sizing in Settings. Lock the card from Settings or its context menu to disable moving and resizing.
- Add an Interface animations preference under Settings → General. On Windows, practice cards ease in and out. A correct answer sends a thin green outline outward, followed by a softer one from the card’s edge. A missed answer draws a soft red outline back toward the card as the correction settles into place. Turning off animations in Try or Windows makes these transitions instant.

### Changed

- On Windows, the main window and image preview use Try’s own title bar and thin border in both themes. The main window groups Back to tray, Maximize or Restore, and Quit Try in the bottom-right corner. The library footer drops the persistent tray practice hint. The floating card has a matching rounded outline.
- The app checks for published stable updates. When the GitHub API rate limit is reached, it checks the repository's latest stable release.
- Put New card beside the pack heading and bulk controls in the table header. Browse packs with the arrow keys and switch between Answer and View at the bottom of the window. Empty libraries offer create and import actions.
- Import and export knowledge packs in the background with progress feedback and a retry action on failure.
- Answers without `answer_latex` use literal text matching. To keep accepting different formula spellings in an older pack, switch its answer field to Formula or declare the accepted spellings as aliases.
- Make keyboard focus easier to see and show answer shortcuts and input errors on practice cards. After grading, the card takes on a green or red tint. A missed fill-in shows the expected answer above your attempt. Choice and true/false cards highlight the correct option in place. When enabled, the dismissal hint stays visible while scrolling through a correction.

### Fixed

- With `slint-math-input 0.1.0-preview.6`, Tab and Shift+Tab move between a logarithm's base, exponent and parenthesized argument in either direction. A square-bracketed expression beside the logarithm keeps its own editing scope.
- Preserve code identifiers, method calls and paths when editing or answering text fields.
- Preserve the original text when switching a field to Formula and back to Text without editing.
- Reject equivalent formula spellings as distractors during import and editing. Saved questions still accept equivalent spellings of the correct answer.
- Support keyboard scrolling on overflowing practice cards and bring unfinished-formula messages into view beneath the answer field, including with the keypad open.
- Retrying a multiple-choice edit after a validation error now drops aliases for the previous correct answer, just like saving the edit on the first attempt.
- Changing the reminder interval, correct-answer duration or knowledge-display duration preserves unfinished quiet-hours edits.

## [0.10.0] - 2026-11-14

### Added

- Add a practice-card context menu to stop practicing the current question, step away for a preset or custom duration, or record a problem locally with the question, input, formula and grading state. Excluded questions retain their content and learning records; automatic closing pauses while the menu is open.

### Changed

- Separate practice-window event bindings, answer handling and timing; share named menu actions and recording states across the UI and application while preserving practice behavior and saved data.

## [0.9.3] - 2026-11-13

### Fixed

- Recognize adjacent coefficients and functions such as `yln(x)` and `ysqrt(x)`, and route compact native commits such as `lnx` through the math editor's expression input.
- Accept explicit or implicit multiplication and redundant scalar parentheses, including recognizable plain answers without `answer_latex`, while preserving number, function-argument and script boundaries.

## [0.9.2] - 2026-11-12

### Changed

- Centralize formula syntax and rendering contracts and separate content-editor internals while preserving knowledge-pack, draft and learning-record compatibility.

### Fixed

- Fix Chinese annotations appearing as missing-glyph boxes in formulas with spacing or preserved structures, including binomial coefficients, nested fields and display-parser decorations; keep native text inside the returned display bounds.

## [0.9.1] - 2026-11-12

### Fixed

- Preserve apostrophe derivative marks such as `u^{'}f^{'}\left(u\right)` when converting structured formulas for display, authoring and answer submission; avoid phantom empty bases while retaining checks for genuinely unfinished formulas.
- Preserve word boundaries when converting formulas back to answer text, so terms typed outside an exponent or subscript stay outside it and are not matched as part of the script.
- Preserve binomial coefficients, products, annotations, matrix styles and postfix factorial scope through formula editing, saving and answer comparison; prevent supported formulas from becoming empty answers or being matched as a different structure.

## [0.9.0] - 2026-11-10

### Added

- Expand typed logarithms and limits into editable fields, navigate fields with Tab/Shift+Tab, exit structures with Space, and undo an automatic expansion back to its literal command.

### Changed

- Organize formula tools into a compact structural palette and nine symbol categories, with matrix construction first, clearer previews and hints, and visible undo/redo availability.

### Removed

- Remove the redundant number, alphabet and on-screen navigation rows. Use the physical keyboard for text, Shift/arrow selection and Tab field navigation; pointer placement remains available.

### Fixed

- Give base-logarithm templates a separate argument and keep keyboard, keypad and pointer input in the intended field, including logarithms with an exponent.
- Start editing function names and unfinished subscripts without waiting for a complete expression; keep typed logarithm bases at the caret and preserve ordinary text answers.
- Keep the division outside the root when typing `sqrt(3)/2`, and preserve existing `log(x)` and `log_2(x)` input while using logarithm templates.
- Update formula navigation through root degrees and operator bounds, and keep keypad power/subscript insertion consistent with typing.
- Accept the component's degree, strict-superset, multiset-union, definition and homomorphism notation throughout saving, display and answer matching. Reset the symbol menu's scroll position when changing categories and complete its English labels.
- Preserve keyboard/pointer selections when expanding or collapsing formula tools, and render binomial previews as stacked coefficients.

## [0.8.2] - 2026-11-07

### Changed

- Complete a first answer pass through participating questions before weighted reviews, including newly added questions and question types without an answer record; viewing an unanswered card does not mark it complete.
- Increase the saturation of correct and incorrect practice feedback in both light and dark themes, with matching answer text and borders.

## [0.8.1] - 2026-11-07

### Fixed

- Update the pinned formula input component so mouse placement before a scripted base keeps inserted coefficients outside it, and clicks beyond a formula's edges stay at the outer boundary instead of entering nested fields.
- Pin the Linux packaging helper to a versioned upstream release so replacements of continuous assets do not interrupt verified builds.

## [0.8.0] - 2026-10-28

### Added

- See the current practice mode in the system tray menu and switch directly between answering and knowledge display; keep it synchronized with the main window and saved preferences.

## [0.7.2] - 2026-10-27

### Fixed

- Keep About visible when opening it from the system tray while the main window receives its initial size updates; re-anchor open information panels when the window is resized.

## [0.7.1] - 2026-10-27

### Fixed

- Save the selected answering or knowledge-display mode immediately and restore it on normal or login startup, instead of resetting to answering. Preserve both saved display durations and default older profiles without a saved mode to answering.

## [0.7.0] - 2026-10-26

### Added

- Set separate durations for correct-answer feedback and knowledge display; existing profiles retain their previous duration for both settings.

### Changed

- Separate window, event-binding and view-model responsibilities, share software-rendered UI test support, and remove unused icons and translations.

## [0.6.0] - 2026-10-25

### Added

- Expand or collapse a knowledge pack's description, with a summary and inline editing below its title.
- Show a red dot on the About icon when an update is available.
- Delete a question directly from its row's action menu.

### Changed

- Reorganize the library into a sidebar and question list, with a visible practice-mode selector, per-question and bulk answer-mode menus, and Settings and About in the lower-left corner.
- Group settings into Practice, Card and General, and refine both themes with separate colors for destructive actions and replacement warnings.
- Center pack and question deletion confirmations over the current page, default to Cancel, support Escape, and keep long content scrollable within the dialog.

### Fixed

- Display each knowledge pack's own enabled state independently of its active question count.

## [0.5.2] - 2026-10-22

### Fixed

- Recheck the latest public release when starting an in-app download, so a cached update hint cannot install an older intermediate version. Distinguish checking from downloading in status and retry messages, and stop without downloading if the check fails, is cancelled, or the release is no longer available.
- Record update installation failures and rollback results in the existing log, and avoid duplicate warnings from update error messages, cancelled downloads and pending edits.
- Enter an exponent as soon as a valid mathematical base is followed by `^`, including questions without a formula hint, so subsequent typing stays inside the exponent while the keypad is collapsed.

## [0.5.1] - 2026-10-21

### Fixed

- Update the pinned formula input component so operators stay inside the active exponent or subscript until the caret explicitly leaves it.
- Allow complete formulas containing multiplication dots (`⋅`, `·`, or `\text{⋅}`) to submit with Enter and match equivalent multiplication notation.

## [0.5.0] - 2026-10-20

### Added

- Support image-only questions with an empty textual prompt, removing the caption and its spacing from practice and fact-viewing cards while keeping a numbered entry in the question list.

### Changed

- Install verified in-app updates and restart automatically after the user starts the download, without a second confirmation. Active edits defer restarting and the update resumes when they are saved or cancelled.

### Fixed

- Update the pinned formula input component to correct vertical caret placement after pasting, Home/End and structural edits, while retaining consecutive vertical navigation and preserving the draft when an edit is rejected.
- Refresh the verified Linux AppImage runtime and its corresponding source archive after the upstream continuous build changed.

## [0.4.1] - 2026-10-19

### Fixed

- Avoid synthesizing Alt key events when activating a Windows window from the tray, preventing that activation path from triggering other applications' global shortcuts.
- Use the same practice input control with the formula keypad open or closed. Start structural editing without opening the keypad, preserve caret movement, selection and undo across toggles, and check complete answers directly with Enter.
- Preserve grouping when importing or pasting complete expressions such as `sqrt(3)/2` into the structural editor, including undo and redo.

## [0.4.0] - 2026-10-16

### Added

- Add structured formula editing to question fields and practice answers, with typeset editing, structural navigation, selection, undo/redo and an inline keypad.
- Collapse or expand the practice keypad with ƒ while retaining unfinished formulas, the caret and editing history. Keep keyboard input available when collapsed; applying a formula does not grade the answer.

### Fixed

- Fix mixed Chinese/math catalog captions and keep practice cards anchored when the keypad expands or collapses. Retain manual positions without saving temporary screen-boundary adjustments.

## [0.3.2] - 2026-10-16

### Added

- On Windows, pass mouse input through while another foreground window hides the cursor or captures the mouse, and restore click interaction when released. Keep the card above newly activated topmost applications without requesting focus.

### Changed

- Rename the setting to **Automatically focus new cards**, preserving its default and saved choice. With it off, keep the complete practice card visible without activating on appearance or hover; click the card to answer.
- Allow background reminders in system busy states such as borderless fullscreen; defer system-reported exclusive Direct3D fullscreen, presentation and lock states. Keep automatic reminder errors quiet.

## [0.3.1] - 2026-10-16

### Changed

- Keep incorrect-answer feedback and the expected answer visible until Enter is pressed; correct answers and fact viewing retain the configured display duration.

## [0.3.0] - 2026-10-15

### Added

- Download updates inside Try with progress, cancellation and retry; apply the verified update with one restart.
- Replace the original Windows executable, complete macOS app bundle or Linux AppImage while reusing the same data directory and launch path.
- Preserve settings, packs, images and learning records. Wait for the updated application to load its profile and start successfully; restore the previous program and profile files if startup fails.
- Keep automatic checks quiet and downloads user initiated.

## [0.2.1] - 2026-10-15

### Added

- Optional PNG/JPEG question images with a compact practice layout and a separate enlarged view, shared by typed answers, multiple choice, true/false and fact viewing.
- Add, replace, remove or drop a picture in the question editor. Try manages its own bounded image files, so moving the original does not break a question.
- Export and import image packs as one self-contained JSON file, with validated resources and shared-picture deduplication. Existing text/formula packs remain compatible.
- Include the image in each answer mode's learning fingerprint while preserving existing text-only history. Update the authoring and user guides for picture questions.

### Fixed

- Fall back to public release checks when GitHub's anonymous API allowance is exhausted, while respecting the configured proxy and excluding draft tags without public downloads. Show a specific message if checking remains limited.

## [0.1.0] - 2026-10-14

### Added

- Frameless practice cards with typed answers, multiple choice, true/false and fact viewing.
- Create, edit, delete, import and export knowledge packs and individual questions, with configurable participation and answer modes. Imported packs are saved independently of their source files; empty packs remain editable.
- Optional pack tags before sidebar titles and on practice cards, with saved visibility and automatic inward positioning.
- Importable 12-question templates for essential calculus formulas, advanced English vocabulary and common Rust APIs, separating each pack's tag from its topic title.
- Native visual formula tools, common expression recognition and typeset math inside the answer field.
- Preserve absolute-value and norm boundaries when parsing fractions, roots and exponents.
- Content-aware card height and scrolling for longer prompts and expected answers.
- Reminder and display settings, temporary pauses, persistent Pause/Resume, daily quiet hours and launch at login.
- Answer cards request keyboard focus by default, with a saved switch to keep focus in the current app.
- Adaptive selection using correct/incorrect results and elapsed time, with memory state per question and answer mode. Fact viewing does not read or update personal learning state.
- Light, dark and system appearance, with System as the default; synchronize the native title bar and open practice cards.
- English and Simplified Chinese UI with saved language preferences.
- A compact About panel with quiet automatic update checks, manual checks and concise third-party notices. Automatic checks are enabled by default, delayed after startup and limited to once per 24 hours.

Windows x64 is provided as a single executable. macOS arm64 and Linux x64 remain experimental; available downloads and platform limitations are listed in the [release notes](docs/release_checklist.md).

[Unreleased]: https://github.com/Ethan-decoy/Try/compare/c54ae2170a9ede713720d2df68dfd8ca936777f5...HEAD
[0.10.0]: https://github.com/Ethan-decoy/Try/compare/e067294199e559e5c09121641f5e01e429b830e8...c54ae2170a9ede713720d2df68dfd8ca936777f5
[0.9.3]: https://github.com/Ethan-decoy/Try/compare/19a9c2a4d147f9b63be85f7289b2050d4b559425...e067294199e559e5c09121641f5e01e429b830e8
[0.9.2]: https://github.com/Ethan-decoy/Try/compare/dcadbe986423058c8a4157f9ac07d30d49558061...19a9c2a4d147f9b63be85f7289b2050d4b559425
[0.9.1]: https://github.com/Ethan-decoy/Try/compare/61e14a0fdcbeb74c39159ab64cf910469f52402d...dcadbe986423058c8a4157f9ac07d30d49558061
[0.9.0]: https://github.com/Ethan-decoy/Try/compare/ce825ec91392d87ef6a1c5f6807b0f6538553790...61e14a0fdcbeb74c39159ab64cf910469f52402d
[0.8.2]: https://github.com/Ethan-decoy/Try/compare/4dbc2a9541cc9989adf8f69176ee070f7c1327cf...ce825ec91392d87ef6a1c5f6807b0f6538553790
[0.8.1]: https://github.com/Ethan-decoy/Try/compare/4f8a0a1b9aca34259d77a82d826dfba754a2dc4f...4dbc2a9541cc9989adf8f69176ee070f7c1327cf
[0.8.0]: https://github.com/Ethan-decoy/Try/compare/c91d99aaa2cdaff27d4e3157d964fe223f6c9cc0...4f8a0a1b9aca34259d77a82d826dfba754a2dc4f
[0.7.2]: https://github.com/Ethan-decoy/Try/compare/ee492486a186dd4f7d447a19c0d06d3c92ba6c6b...c91d99aaa2cdaff27d4e3157d964fe223f6c9cc0
[0.7.1]: https://github.com/Ethan-decoy/Try/compare/40b7cc523d07a4fb04cf6e28ac317905723a7596...ee492486a186dd4f7d447a19c0d06d3c92ba6c6b
[0.7.0]: https://github.com/Ethan-decoy/Try/compare/cbb964675599ef7c9c3b1ecd90aea3eedce80530...40b7cc523d07a4fb04cf6e28ac317905723a7596
[0.6.0]: https://github.com/Ethan-decoy/Try/compare/278e50a4b335ff78b605d83dc175e0d27b2f9310...cbb964675599ef7c9c3b1ecd90aea3eedce80530
[0.5.2]: https://github.com/Ethan-decoy/Try/compare/9d4e3a7cd1079d996f284296ac9dca10314f3095...278e50a4b335ff78b605d83dc175e0d27b2f9310
[0.5.1]: https://github.com/Ethan-decoy/Try/compare/b25b2e52e940d907b583583c72b10f0cc1d876c3...9d4e3a7cd1079d996f284296ac9dca10314f3095
[0.5.0]: https://github.com/Ethan-decoy/Try/compare/f6936a4a185bd1c65764b371073e40b63636e21f...b25b2e52e940d907b583583c72b10f0cc1d876c3
[0.4.1]: https://github.com/Ethan-decoy/Try/compare/659a1f147fb6c80eb44d4af9f7d54ecdbe59402b...f6936a4a185bd1c65764b371073e40b63636e21f
[0.4.0]: https://github.com/Ethan-decoy/Try/compare/bb20f2f9b0d57515427e7c991c4baec123287f3e...659a1f147fb6c80eb44d4af9f7d54ecdbe59402b
[0.3.2]: https://github.com/Ethan-decoy/Try/compare/417218acc3ce26440b6fc3ee5315fdbd87412db4...bb20f2f9b0d57515427e7c991c4baec123287f3e
[0.3.1]: https://github.com/Ethan-decoy/Try/compare/b42f801ac2132add00d35421a0f752c0841e4357...417218acc3ce26440b6fc3ee5315fdbd87412db4
[0.3.0]: https://github.com/Ethan-decoy/Try/compare/33e87365e939b124e454f3fe2d075eaf54c5c372...b42f801ac2132add00d35421a0f752c0841e4357
[0.2.1]: https://github.com/Ethan-decoy/Try/compare/3c09e19272c5db71cf7369cd4e173dba52fa071b...33e87365e939b124e454f3fe2d075eaf54c5c372
[0.1.0]: https://github.com/Ethan-decoy/Try/commit/3c09e19272c5db71cf7369cd4e173dba52fa071b
