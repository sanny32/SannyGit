# User Interface
- Keep the OS-provided title bar and window controls. Do not replace them with
  custom controls in the application toolbar.
- All dialogs except Settings must use the shared header from
  `src/ui/controls/dialog_style.rs` (`header` / `header_with_icon`): the standard
  title, description, icon, typography, background and spacing. Keep this header
  consistent when making dialogs more compact; do not replace it with a custom
  heading. Settings retains its own layout.
- Preserve the traditional Working Tree layout: toolbar at the top, repositories
  and branches on the left, changes and diff in the center, commit and journal
  panels below, and a status bar along the bottom edge.
- Favor compact controls, high information density, resizable panels, and clear
  selection states. Avoid oversized cards and unnecessary decoration.
- Content must not jump during background loading, refreshes, progress updates,
  or errors. Preserve existing rows, scroll position, and control placement;
  show transient status without inserting or removing layout rows. Geometry
  should change only for an explicit user layout action or actual content changes.
- Dialog sizes may change only when collapsed sections are expanded. Loading,
  progress, errors, and action button clicks must not resize dialogs; reserve
  compact, stable space and keep overflowing text accessible (for example, in
  a tooltip). Do not stabilize dialog sizes by adding large empty areas.
- For every operation launched from a dialog, close the initiating form or
  confirmation immediately after the final action starts and run the operation
  in the background. Show progress and success in the status bar. Do not open a
  progress window, a second confirmation, or a success dialog.
- Display all user-facing error messages in modal dialogs, including errors from
  background tasks, settings, previews, and repository operations. Do not show
  error details inline in panels, status bars, tooltips, or operation dialogs.
  Keep the underlying layout stable and preserve drafts and retry controls.
- Support both themes. Define colors through the palette in `src/theme.rs`
  rather than arbitrary constants inside components.
- Use the existing SVG icons and `src/ui/controls/icons.rs`.
- Disable unimplemented actions with a clear tooltip, or hide them. Do not create
  controls that appear functional but do nothing.
- Commits and other repository operations must be initiated by an explicit
  action from the application user. Never commit automatically.
- Check the layout in both themes at the normal window size and the minimum
  size of 1100 x 700. Do not display the compiler version in the status bar.
- All keyboard shortcuts must match SmartGit (Working Tree) for the same action.
  Defaults and user overrides live in `src/shortcuts.rs`; the Preferences
  Keyboard Shortcuts page edits overrides. When adding or changing a default,
  compare it with SmartGit.
- Every action in the application menu must appear in that Keyboard Shortcuts
  list, including actions that have no default shortcut. Add the command to the
  `src/shortcuts.rs` catalog in the same change that adds the menu item. Leave
  unimplemented placeholders out until they become real actions. The catalog
  test `catalog_lists_every_menu_action_even_without_a_shortcut` checks this.

