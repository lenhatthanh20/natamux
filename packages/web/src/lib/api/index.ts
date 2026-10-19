import { createClient } from './client'
import { filesApi } from './files'
import { gitApi } from './git'
import { reposApi } from './repos'
import { stagesApi } from './stages'
import { tasksApi } from './tasks'
import { terminalsApi } from './terminals'
import type { FetchFunction } from './types'

export { ApiError, isConflict, isUnauthorized } from './errors'
export { readTaskActivity } from './parse'
export type {
  AgentActivity,
  BranchList,
  FetchFunction,
  LoadedSettings,
  NewWorkspaceInput,
  RemoveWorkspaceResult,
  SavedSettings,
  SendEventResult,
  StageAction,
  StageActionInput,
  StageFile,
  StoredEvent,
  TaskActivity,
  TaskChanges,
  TaskShells,
} from './types'

export function createApi(
  token: string,
  fetchFunction: FetchFunction = (input, init) => fetch(input, init),
) {
  const client = createClient(token, fetchFunction)
  return {
    ...tasksApi(client),
    ...terminalsApi(client),
    ...stagesApi(client),
    ...reposApi(client),
    ...filesApi(client),
    ...gitApi(client),
  }
}

export type Api = ReturnType<typeof createApi>
