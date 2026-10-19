# Changelog

Notable changes are recorded here using [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and [Semantic Versioning](https://semver.org/spec/v2.0.0.html). See the [release policy](docs/release_policy.md) for Try's conventions during `0.x` development. Versions through `0.4.0` were published as GitHub prereleases.

## [Unreleased]

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

[Unreleased]: https://github.com/Ethan-decoy/Try/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/Ethan-decoy/Try/compare/v0.3.2...v0.4.0
[0.3.2]: https://github.com/Ethan-decoy/Try/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/Ethan-decoy/Try/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Ethan-decoy/Try/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/Ethan-decoy/Try/compare/v0.1.0...v0.2.1
[0.1.0]: https://github.com/Ethan-decoy/Try/releases/tag/v0.1.0
