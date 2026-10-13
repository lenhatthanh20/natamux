---
title: Keyboard shortcuts
description: Every keyboard shortcut of the Natamux dashboard, grouped by area.
sidebar:
  order: 4
---

Press `?` in the dashboard, or click the keyboard icon in the status bar, to see this list inside the app. It has a **Search the shortcuts** field.

## Modifier keys

| In this page | macOS | Windows and Linux |
|---|---|---|
| `Mod` | ⌘ | Ctrl |
| `Alt` | ⌥ | Alt |
| `Shift` | ⇧ | Shift |
| `Ctrl` | Ctrl | Ctrl |

The dashboard shows shortcuts in tooltips and menus with the keys of your platform. ``Ctrl+Shift+` `` (below) is Ctrl on every platform.

## Where keys work

- **Single keys** such as `n` or `c` work only when the focus is on the page, not in a text field and not in a terminal. They would otherwise type a letter.
- **Chords** such as `Mod+K` work everywhere, except as the next point says.
- **In a terminal**, the shell owns the keys. On macOS the ⌘ chords still reach Natamux. On Windows and Linux only function keys and chords with `Ctrl+Shift` reach Natamux. `Ctrl+K`, `Ctrl+B` and `Ctrl+J` go to the shell.
- When a dialog, menu or the palette is open, it owns the keyboard.

## Global

| Action | Shortcut |
|---|---|
| Open the command palette | `Mod+K` or `F1` |
| Jump to a workspace | `Mod+J` |
| Show or hide the sidebar | `Mod+B` |
| Show or hide the details | `Mod+Alt+B` |
| Move to the next part of the window | `F6` |
| Move to the previous part of the window | `Shift+F6` |
| Submit the current form | `Mod+Enter` |
| Close the top layer, clear a field, or go back to All tasks | `Esc` |

`Mod+Enter` submits the form in a dialog, the commit box, the comment box and the task edit form. The primary button shows the shortcut.

`Esc` blurs a text field first. Then it closes the open workspace or the Agents view. Then it closes the details of the selected task.

## Single keys

| Action | Shortcut | Works when |
|---|---|---|
| Show the keyboard shortcuts | `?` | Anywhere outside a field |
| Search the sidebar | `/` | Anywhere outside a field |
| Start a task | `c` | Anywhere outside a field |
| Add a task | `n` | Anywhere outside a field |
| Select the next task | `j` or `↓` | The task list is shown |
| Select the previous task | `k` or `↑` | The task list is shown |
| Open the selected task | `Enter` | A task is selected and the focus is on the page |
| Show or hide the details of the selected task | `i` | A task is selected |
| Edit the selected task | `e` | A task is selected |
| Approve | `a` | The selected task is in review |
| Request changes | `r` | The selected task is in review |
| Show the Terminal tab | `1` | A workspace is open |
| Show the Changes tab | `2` | A workspace is open |
| Show the History tab | `3` | A workspace is open |
| Show the Docs tab | `4` | A workspace is open |
| Read the changes and the history again | `Shift+R` | A workspace is open |

`n` closes the open workspace and puts the cursor in the **Add a task to the backlog (n)** field.

## Lists and trees

These keys work in the sidebar tree, the task table and the changed files list.

| Action | Shortcut |
|---|---|
| Move up or down | `↑` `↓` (and `j` `k` in the task table) |
| First or last row | `Home` `End` |
| Collapse or expand a group | `←` `→` |
| Rename the workspace | `F2` |
| Open the menu of the row | `Shift+F10` or `Menu` |
| Remove the workspace (asks first) | `Delete` |

`F2`, `Delete` and the row menu are for workspace rows in the sidebar. In the task table, `Enter` opens a task and `i` shows its details.

## Project settings

These work in the **Stages** tab of the project settings, with the focus on a stage in the list.

| Action | Shortcut |
|---|---|
| Move the stage up | `Alt+ArrowUp` |
| Move the stage down | `Alt+ArrowDown` |
| Save the settings | `Mod+Enter` |

The row menu has the same moves as **Move up** and **Move down**. See [Project settings](/docs/guides/dashboard/#project-settings).

## Panes and terminals

| Action | Shortcut |
|---|---|
| Maximize or restore the focused pane | `Mod+Shift+Enter` |
| New shell | ``Ctrl+Shift+` `` |
| Find in the terminal | `Mod+F` or `Mod+Shift+F` |

`Mod+F` and `Mod+Shift+F` work when the focus is in a terminal. In the find bar, `Enter` goes to the next match, `Shift+Enter` to the previous one and `Esc` closes the bar. On Windows and Linux use `Ctrl+Shift+F` in a terminal.

See [Terminal](/docs/guides/terminal/).

## Diff

These work when a workspace is open.

| Action | Shortcut |
|---|---|
| Previous changed file | `[` |
| Next changed file | `]` |
| Next change in the file | `Alt+F5` |
| Previous change in the file | `Shift+Alt+F5` |
| Comment on the file for the agent | `Mod+Shift+A` |
| Draft the commit message with the agent | `Mod+Shift+G` |
| Accessible diff view | `F7` or `Shift+F7` |

`Mod+Shift+A` puts the cursor in the comment box. In that box, `Mod+Enter` sends the comment. `F7` is handled by the diff editor, so it works while the focus is in the diff. See [Changes and history](/docs/guides/changes-and-history/).

## Command palette

| Action | Shortcut |
|---|---|
| Move in the list | `↑` `↓` |
| Open the row, or add it to Agents | `Enter`, `Shift+Enter` |
| Pick row 1 to 9 | `Alt+1` to `Alt+9` |
| Change the scope | `Tab` |
| Close | `Esc` |

In the palette, type `>` to search commands only and `#` to search tasks by id. See [Dashboard](/docs/guides/dashboard/#command-palette).

## Commands without a shortcut

Some commands are only in the palette (`Mod+K`, then type the name):

- **All tasks**, **Agents**
- **Show tasks as a table**, **Show tasks as a board**, **Show tasks against their stages**
- **Follow the system**, **Light**, **Dark** (theme)
- **Text size: Compact**, **Default**, **Large**
- **Add a project**, **Import GitHub issues**
- **Show the task progress**, for the open or selected task (opens the details on **Progress**)
- **Mark ready**, **Block**, **Resume the agent**, **Unblock**, **Retry**, for the selected task
- **Skip the stage**, **Mark the stage as done**, **Approve the stage anyway**, **Send the task back to an earlier stage**, for the selected task in a workspace
