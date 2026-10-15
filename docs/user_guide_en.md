# Try user guide

[简体中文](user_guide.md) · [Home](../readme_en.md)

This guide covers the `0.2.1` prerelease for Windows x64, macOS arm64 and Linux x64. Platform packages and verification scope are in the [release record](release_checklist.md).

## Practice

Place the Windows executable in a writable folder and open `try.exe`. Closing the main window returns to the tray. Quit from the tray menu or the power icon in the main window to exit completely.

Click the tray icon on Windows/Linux to start a card. On macOS, choose Practice from the menu-bar icon. The menu also opens Settings or About, pauses automatic practice, resumes it, or quits. Mac/Linux remain experimental; complete tray, focus and file-dialog behavior needs desktop testing.

Enter submits a typed answer. Click a multiple-choice option or use 1, 2, 3; click ✓ / ✕ for true/false. Green means correct; red reveals the expected answer. Esc closes the card, and Enter also closes feedback. Closing without submitting does not record a result.

Scheduled answer cards request keyboard focus by default, so you can type or use the number keys immediately. Turn off **Focus new practice cards** at the end of Settings to keep the other window focused until you click the card or start practice manually. The preference persists through restarting; fact viewing does not take keyboard focus automatically.

Display duration starts after an answer or when a fact is shown. Unanswered cards stay open. Automatic cards pause while the main window is open; the full interval restarts after returning to the tray and finishing practice.

The lower-left logo switches between answering (white) and viewing facts (outlined). Facts close after the configured duration or with Enter/Esc. Restarting returns to answering mode.

Answering adjusts selection from correct/incorrect results and actual elapsed time. Missed items get more revisits with other questions in between, while new and older items keep rotation opportunities. Familiar items still get maintenance within your chosen reminder interval. Typed, multiple-choice and true/false records remain separate. Fact viewing uses ordinary selection without reading or updating learning records; returning to answering retains the previous records.

## Packs and questions

Configured tags also appear before sidebar pack names. The sidebar and practice card share the Show pack tags setting. Long tags are shortened; hover to read the full text.

Expand a pack's title to edit a short tag, such as Formulas or English. Choose a tag and name that work together, such as Formulas · Calculus or English · Words, without repeating the same information. Enter saves and Esc cancels; a blank tag stays hidden. Tags are included in pack exports. A card on the left uses its upper-right corner; a card on the right uses its upper-left corner, updating as the card moves. Settings > Show pack tags hides all tags and preserves your preference after restarting. Existing packs without a tag remain unchanged; add one if desired.

Choose knowledge you want to recall at a glance, then break it into brief cues with clear answers that need little on-the-spot reasoning.

- Click a sidebar name to browse a pack. Its switch controls participation; several packs can be enabled together. Disabling and re-enabling preserves selected questions.
- Row switches control individual questions; All controls the whole pack. Select typed, multiple-choice or true/false cells per question or in bulk. A mode with missing content is unavailable until the question is edited.
- The sidebar plus creates a pack. The down/up arrows import/export JSON; hover for labels. The content-header plus adds a question, the pencil renames the pack, and the arrow expands its description. Enter saves a name; Esc cancels.
- Click a question to edit it. Typed mode needs a prompt and answer; multiple choice needs three options and a correct selection; true/false needs a statement and ✓ / ✕. Save validates the complete question; ordinary Cancel keeps the saved question.
- To delete one question, use the upper-right trash icon in its editor and confirm. Canceling deletion retains the draft. The last deletion leaves an empty pack for new questions or imports.
- To delete a pack, use the trash icon beside its title and confirm. Export first if you need a backup. Deleting the last pack leaves an empty library; restarting does not restore deleted content.

Import copies the complete pack into Try's local data. Moving, renaming or deleting the source JSON does not affect it. External edits require re-importing; replacing the same id asks for confirmation. In-app edits and deletions do not change the source file. Export contains pack content; reminder settings, participation and per-question modes are local preferences.

Adding to a partially selected pack leaves new questions unselected. Adding to a fully selected pack includes them. An empty pack does not produce cards; adding questions retains its enabled state. Authoring resources: [format](knowledge_pack_format.md), [AI instructions](knowledge_pack_ai_template.md). Three advanced templates contain 12 questions each, with a complementary tag and name: [Formulas · Calculus](../knowledge_packs/authoring_template.json), [English · Advanced words](../knowledge_packs/vocabulary_template.json) and [Rust · Common patterns](../knowledge_packs/rust_template.json). Their tags and names use Chinese except for Rust, preserving the authored content when you change the UI language.

### Starter examples

A fresh profile includes five small packs with four questions each. Edit or delete them, or enable only the topics you want to practice. These examples currently use Chinese prompts; changing the UI language preserves authored content.

| Topic and importable JSON | Example cue | Answer |
| --- | --- | --- |
| [Math](../knowledge_packs/math_recall.json) | `(sin x)′ = ?` | `cos(x)` |
| [English](../knowledge_packs/english_recall.json) | `hesitate` | 犹豫 |
| [Programming](../knowledge_packs/programming_recall.json) | Rust: mutable reference | `&mut T` |
| [Chemistry](../knowledge_packs/chemistry_recall.json) | Benzene: π electrons | `6` |
| [Everyday facts](../knowledge_packs/general_recall.json) | `1 nautical mile (metres)` | `1852` |

Updating retains existing packs and preferences without adding starter content or restoring deletions. Import the JSON files above if desired.

## Picture prompts

Try 0.2.0 supports one static PNG or JPEG per question. Version 0.1.0 does not include this feature. In the question editor, choose **Add image** to select a file, or drop an image file onto the main window while editing a question. You can replace or remove an existing picture, and click the preview to enlarge it. Save commits the complete question; cancel preserves its original picture.

Practice uses the compact card: short prompt, picture, then the answer controls. Pictures keep their colors and aspect ratio, and work with input, multiple choice, true/false and knowledge display. Click a picture to open a larger window; Esc closes that window. Automatic dismissal pauses while viewing the enlarged picture; continue answering or use Enter/Esc to dismiss the card afterwards.

Try copies images into its own `media/` data folder, so moving or deleting the original file does not affect the question. Source files are limited to 8 MiB, 12 million pixels and 8192 pixels per dimension. Background processing resizes the display image to at most 524288 pixels. Each pack can use up to 128 distinct images totaling 16 MiB after processing. Shared pictures are removed only when no question or active draft references them.

Export still produces one JSON file, with the pictures included. Importing it on another computer restores them. Existing text/formula packs remain compatible; picture packs require Try 0.2.0 or later. Preserve the complete Windows portable `data/` folder, including `media/`, when moving the app. Missing or damaged pictures produce an error instead of a wrong-answer result.

## Formulas

Type common expressions such as `sin(pi/3)`, `sqrt(3)/2`, or supported LaTeX. Complete formulas are typeset after a brief typing pause. Ordinary text and incomplete drafts such as `sqrt(` remain editable text. Submitting an unfinished formula in a math question keeps the draft open with a completion hint.

Click a numerator, denominator or exponent to position the caret. Arrow keys move through layout; Shift plus arrows or dragging selects. Clipboard operations retain source input, and Ctrl+Z / Ctrl+Y undo and redo. Normalization supports written forms without arbitrary algebraic simplification or approximate numeric comparison.

ƒ opens formula tools. The editor supports functions, symbols, fractions, roots, powers, integrals, sums, limits, matrices, cases and mixed text/math. Click the preview to edit a parameter or cell. The answer pad offers common structures and numeric operators: ✓ or Enter submits, ƒ applies and closes the pad, and Esc cancels pad edits.

The move-out and unwrap controls adjust scope while retaining contents. C clears the current field; undo restores the previous draft and position. Clearing does not submit. Structural tools use the caret and selection; ↑ selects the outer structure and ↗ continues after it. Empty templates can be revisited.

## Settings and pauses

Open Settings from the lower-left gear. Changes save automatically; a save failure retains valid data. Appearance offers Light, Dark and System, with System as the default. The main window, native title bar and practice card update together, and System follows changes to the operating system's appearance. Your choice persists through restarting; existing saved Light or Dark choices remain valid. Choose system language, Simplified Chinese or English under Language at the bottom. Pack content keeps its authored language.

Interval presets are 15, 25, 30 and 45 minutes; feedback presets are 1, 2 and 3 seconds. Use the adjacent precise controls for custom values: Enter applies and Esc cancels. Existing custom values are retained.

A temporary pause lasts for its selected duration; Pause lasts until Resume. Pauses survive restarting. Resume starts the full interval without catching up missed cards. Manual practice works during pauses and quiet hours and does not resume automatic reminders.

Daily quiet hours and launch at login are off by default. Quiet hours can span midnight, with 22:00–08:00 as the initial range. Launch at login starts in the tray. On Windows, choose a work-area corner or drag the card's prompt/empty area to position it.

## Data, updates and feedback

Windows distribution builds create `data/` beside the executable, with settings and packs in `state.json` and personal practice state in `learning_state.json`. Existing profiles without history start as unassessed; fact viewing does not create learning records. Pack exports contain no personal history. Mac/Linux use system application data folders. Development profiles and custom directory arguments are in the [development guide](development.md).

Quit before updating, replace `try.exe` and keep `data/`. Move both together when moving the portable app. If launch at login was enabled, turn it off and on at the new location to update the startup entry.

The lower-left ⓘ opens About for version, manual update checks, homepage and notices. Esc or clicking outside closes it. Complete license information is in [licensing](licensing.md).

Automatic update checks are on by default. They wait at least 30 seconds after startup and run at most once every 24 hours, including across restarts. An available version appears only in About, without a popup. Turn off **Check for updates automatically** at the bottom of Settings if preferred; manual checking remains available. The check time and last successful result are cached in `update_check_state.json` in the same data folder. Download and replace the application yourself to update it.

Report problems or suggest improvements through [Issues](https://github.com/Ethan-decoy/Try/issues).
