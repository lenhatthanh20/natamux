import type { Task } from '@natamux/core'
import type { RefObject } from 'react'
import { fireAppEvent } from '../../lib/app-events'
import type { CommandActions, CommandState } from '../../lib/commands'
import { focusKind } from '../../lib/dispatch'
import { agentPaneTasks, MAX_PANES } from '../../lib/panes'
import type { TaskAction } from '../../lib/tasks'
import { toast } from '../../lib/toasts'
import { useViewStore } from '../../lib/view-store'
import { tabToOpen } from '../../lib/workspace'
import type { PaletteMode } from '../palette/CommandPalette'
import type { PaletteScope } from '../palette/palette'
import type { useDashboardLayout } from './useDashboardLayout'
import { useKeyboard } from './useKeyboard'
import type { useTaskMutations } from './useTaskMutations'

type View = ReturnType<typeof useViewStore.getState>

type CommandsInput = Pick<
  View,
  'selectedId' | 'sidebarOpen' | 'drawerOpen' | 'changesTaskId' | 'agentsOpen' | 'agentPanes'
> &
  Pick<
    ReturnType<typeof useDashboardLayout>,
    | 'frozen'
    | 'orderedTasks'
    | 'selectedTask'
    | 'changesTask'
    | 'detailTask'
    | 'shellLayout'
    | 'detailsOpenOf'
    | 'detailOpen'
  > &
  Pick<ReturnType<typeof useTaskMutations>, 'eventMutation' | 'stageMutation' | 'setRemovingId'> & {
    tasks: Task[]
    noProjects: boolean
    searchRef: RefObject<HTMLInputElement | null>
    newTaskRef: RefObject<HTMLInputElement | null>
    setNewWorkspace: (input: { repo?: string; title?: string }) => void
    setPalette: (palette: { mode: PaletteMode; scope: PaletteScope; query: string }) => void
    setShortcutsOpen: (open: boolean) => void
    setSettingsOpen: (open: boolean) => void
    setReposOpen: (open: boolean) => void
    setImportOpen: (open: boolean) => void
  }

// What the user can do from the keys, the palette, the menus and the lists. Runs useKeyboard.
export function useDashboardCommands({
  selectedId,
  sidebarOpen,
  drawerOpen,
  changesTaskId,
  agentsOpen,
  agentPanes,
  frozen,
  orderedTasks,
  selectedTask,
  changesTask,
  detailTask,
  shellLayout,
  detailsOpenOf,
  detailOpen,
  eventMutation,
  stageMutation,
  setRemovingId,
  tasks,
  noProjects,
  searchRef,
  newTaskRef,
  setNewWorkspace,
  setPalette,
  setShortcutsOpen,
  setSettingsOpen,
  setReposOpen,
  setImportOpen,
}: CommandsInput) {
  const {
    select,
    setDetailOpen,
    showProgress,
    showFiles,
    setSidebarOpen,
    setDrawerOpen,
    openChanges,
    closeChanges,
    setLayout,
    openAgents,
    setAgentPanes,
  } = useViewStore.getState()

  // With no project there is nowhere to make a workspace: the projects panel opens instead.
  const openNewWorkspace = (input: { repo?: string; title?: string }) => {
    if (noProjects) {
      toast('Add a project first.')
      setReposOpen(true)
      return
    }
    setNewWorkspace(input)
  }

  // The dialog sends the user to the changes of the task, to commit or discard them.
  const showChangesOf = (taskId: string, commit: boolean) => {
    setRemovingId(null)
    select(taskId)
    openChanges(taskId, 'changes')
    if (commit) {
      setTimeout(() => document.getElementById('commit-message')?.focus(), 80)
    }
  }

  const runAction = (task: Task, action: TaskAction) => {
    eventMutation.mutate({ taskId: task.id, type: action.type })
  }

  // A pick in the list shows the task's details, as a sheet or a page where there is no room.
  const chooseTask = (id: string) => {
    select(id)
    setDetailOpen(id, true)
  }

  // A drawer closes when the user picks something in it.
  const afterPick = () => {
    if (shellLayout.sidebar === 'drawer') {
      setDrawerOpen(false)
    }
  }

  const toggleSidebar = () => {
    if (shellLayout.sidebar === 'drawer') {
      setDrawerOpen(!drawerOpen)
    } else {
      setSidebarOpen(!sidebarOpen)
    }
  }

  const toggleDetails = () => {
    if (detailTask !== null) {
      setDetailOpen(detailTask.id, !detailOpen)
    }
  }

  // A stage file opens in the Docs tab of the task's workspace, which may have to open first.
  const openStageFile = (task: Task, name: string) => {
    if (changesTask?.id === task.id) {
      fireAppEvent('open-doc', name)
      return
    }
    openChanges(task.id, 'docs')
    // The workspace is drawn first, then it hears the request.
    setTimeout(() => fireAppEvent('open-doc', name), 80)
  }

  // Shows the sidebar if it is hidden, then runs `then` once it is on screen.
  const revealSidebar = (then: () => void) => {
    if (shellLayout.sidebar === 'drawer') {
      setDrawerOpen(true)
    } else if (!sidebarOpen) {
      setSidebarOpen(true)
    }
    requestAnimationFrame(then)
  }
  const focusFilter = () => revealSidebar(() => searchRef.current?.focus())

  // Opens the task's workspace in the main area: the agent's terminal while it works, else the
  // tab that was used last there. The details keep the state they had.
  const openTask = (id: string) => {
    const task = tasks.find((candidate) => candidate.id === id)
    select(id)
    const lastTab = useViewStore.getState().tabByTask[id]
    openChanges(id, task === undefined ? 'terminal' : tabToOpen(task.state, lastTab))
  }

  const moveSelection = (step: number) => {
    if (orderedTasks.length === 0) {
      return
    }
    const current = orderedTasks.findIndex((task) => task.id === selectedId)
    const next = Math.min(Math.max(current + step, 0), orderedTasks.length - 1)
    const target = orderedTasks[next]
    if (target) {
      select(target.id)
      frozen.hold()
    }
  }

  const addToAgents = (id: string) => {
    const chosen = agentPaneTasks(agentPanes, tasks).map((task) => task.id)
    if (!chosen.includes(id)) {
      if (chosen.length >= MAX_PANES) {
        toast(`The Agents grid holds ${MAX_PANES} panes. Close one to add ${id}.`, 'error')
      } else {
        setAgentPanes([...chosen, id])
      }
    }
    openAgents()
    afterPick()
  }

  const commandState: Omit<CommandState, 'focus'> = {
    workspaceOpen: changesTask !== null,
    agentsOpen,
    hasSelection: selectedTask !== null,
    selectedState: selectedTask?.state ?? null,
  }

  // Everything a command can do. The keys, the palette and the menus all go through it.
  const actions: CommandActions = {
    openPalette: (query = '') => setPalette({ mode: 'all', scope: null, query }),
    openJump: (scope = undefined) => setPalette({ mode: 'jump', scope: scope ?? null, query: '' }),
    toggleSidebar,
    toggleDetails,
    showShortcuts: () => setShortcutsOpen(true),
    openSettings: () => setSettingsOpen(true),
    focusFilter,
    newWorkspace: () => openNewWorkspace({}),
    newTask: () => {
      closeChanges()
      // The field is in the task list, which may only now be shown.
      requestAnimationFrame(() => newTaskRef.current?.focus())
    },
    moveSelection,
    openSelected: () => {
      if (selectedTask) {
        openTask(selectedTask.id)
      }
    },
    toggleSelectedDetails: () => {
      if (selectedTask) {
        setDetailOpen(selectedTask.id, !detailsOpenOf(selectedTask.id))
      }
    },
    editSelected: () => {
      if (selectedTask === null) {
        return
      }
      const shown = detailOpen && detailTask?.id === selectedTask.id
      setDetailOpen(selectedTask.id, true)
      // The details are drawn first, then they hear the request.
      setTimeout(() => fireAppEvent('edit-task'), shown ? 0 : 80)
    },
    taskEvent: (type) => {
      if (selectedTask) {
        eventMutation.mutate({ taskId: selectedTask.id, type })
      }
    },
    stageAction: (action) => {
      if (selectedTask) {
        stageMutation.mutate({ taskId: selectedTask.id, action })
      }
    },
    showTab: (tab) => fireAppEvent('workspace-tab', tab),
    showProgress: () => {
      if (detailTask) {
        showProgress(detailTask.id)
      }
    },
    showFiles: () => {
      if (detailTask && detailTask.workspace !== null) {
        showFiles(detailTask.id)
      }
    },
    back: () => {
      const focused = document.activeElement
      if (focused instanceof HTMLElement && focusKind(focused) === 'field') {
        focused.blur()
      } else if (changesTaskId !== null || agentsOpen) {
        closeChanges()
      } else if (selectedTask) {
        setDetailOpen(selectedTask.id, false)
      }
    },
    showAllTasks: closeChanges,
    showAgents: openAgents,
    setLayout: (next) => {
      closeChanges()
      setLayout(next)
    },
    openRepos: () => setReposOpen(true),
    openImport: () => setImportOpen(true),
  }

  useKeyboard({ state: commandState, actions })

  return {
    openNewWorkspace,
    showChangesOf,
    runAction,
    chooseTask,
    afterPick,
    toggleSidebar,
    toggleDetails,
    openStageFile,
    openTask,
    addToAgents,
    commandState,
    actions,
  }
}
