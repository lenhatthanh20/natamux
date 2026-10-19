import type { Task } from '@natamux/core'
import type { KeyboardEvent } from 'react'
import { firstChildKey, parentKey } from '../../lib/roving'
import type { RepoGroup } from '../../lib/workspace-groups'
import type { Entry } from './sidebar-prefs'
import type { shellRows } from './sidebar-tree'

type TreeSource = {
  pinned: readonly Task[]
  groups: readonly RepoGroup[]
  children: Record<string, Task[]>
  repoOpen: (id: string) => boolean
  finishedShown: (id: string) => boolean
  taskOpen: (task: Task) => boolean
  canFold: (task: Task) => boolean
  rowsOf: (task: Task) => ReturnType<typeof shellRows>
  waitingChildren: (task: Task) => Task[]
  setFold: (key: string, open: boolean) => void
  onOpen: (id: string) => void
}

// The rows as they are on screen, for the keyboard.
export function treeEntries({
  pinned,
  groups,
  children,
  repoOpen,
  finishedShown,
  taskOpen,
  canFold,
  rowsOf,
  waitingChildren,
  setFold,
  onOpen,
}: TreeSource): Entry[] {
  const entries: Entry[] = []
  const pushTask = (task: Task, parent: string | null) => {
    const key = `task:${task.id}`
    const open = taskOpen(task)
    entries.push({
      key,
      parent,
      expandable: canFold(task),
      label: task.title,
      open,
      // A folded task opens when it becomes the open workspace; the open one does not fold.
      toggle: open ? undefined : () => onOpen(task.id),
    })
    if (open) {
      for (const shell of rowsOf(task)) {
        entries.push({
          key: `agent:${task.id}:${shell.key}`,
          parent: key,
          expandable: false,
          label: shell.label,
        })
      }
    }
    for (const child of open ? (children[task.id] ?? []) : waitingChildren(task)) {
      entries.push({ key: `task:${child.id}`, parent: key, expandable: false, label: child.title })
    }
  }
  for (const task of pinned) {
    pushTask(task, null)
  }
  for (const { repo, open, finished } of groups) {
    const repoKey = `repo:${repo.id}`
    const shown = repoOpen(repo.id)
    entries.push({
      key: repoKey,
      parent: null,
      expandable: true,
      label: repo.name,
      open: shown,
      toggle: () => setFold(repoKey, !shown),
    })
    if (!shown) {
      continue
    }
    for (const task of open) {
      pushTask(task, repoKey)
    }
    if (finished.length > 0) {
      const finishedKey = `finished:${repo.id}`
      const finishedOpen = finishedShown(repo.id)
      entries.push({
        key: finishedKey,
        parent: repoKey,
        expandable: true,
        label: '',
        open: finishedOpen,
        toggle: () => setFold(finishedKey, !finishedOpen),
      })
      if (finishedOpen) {
        for (const task of finished) {
          pushTask(task, finishedKey)
        }
      }
    }
  }
  return entries
}

// Right unfolds or steps into the children; Left folds or steps to the parent.
export function treeKey(
  event: KeyboardEvent<HTMLElement>,
  key: string,
  entries: readonly Entry[],
  focusKey: (key: string) => void,
): boolean {
  const entry = entries.find((candidate) => candidate.key === key)
  const expandable = entry?.expandable === true
  const isOpen = entry?.open === true
  if (event.key === 'ArrowRight' && expandable) {
    if (!isOpen) {
      entry?.toggle?.()
    } else {
      const child = firstChildKey(entries, key)
      if (child !== null) {
        focusKey(child)
      }
    }
    return true
  }
  if (event.key === 'ArrowLeft') {
    if (expandable && isOpen && entry?.toggle !== undefined) {
      entry?.toggle?.()
      return true
    }
    const parent = parentKey(entries, key)
    if (parent !== null) {
      focusKey(parent)
      return true
    }
  }
  return false
}
