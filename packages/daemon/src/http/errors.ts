import type { FastifyReply, FastifyRequest } from 'fastify'
import { z } from 'zod'
import { UnknownAgentError } from '../agents/agent-hook-service.js'
import { TaskWaitingError } from '../agents/agent-runner.js'
import { NothingToCommitError } from '../git/commit-push.js'
import { GitError, OutsideWorkspaceError } from '../git/exec.js'
import { ImageSideMissingError, ImageTooLargeError, NotAnImageError } from '../git/file-diff.js'
import { GitHubError } from '../git/gh.js'
import { IssueLinkError } from '../github/issue-linker.js'
import { SettingsChangedError, SettingsInputError } from '../projects/project-settings.js'
import { RepoInputError, RepoInUseError, RepoNotFoundError } from '../projects/repo-service.js'
import { StageActionError, StageInputError } from '../stages/stage-actions.js'
import { StageFileError } from '../stages/stage-files.js'
import { AttachmentError } from '../tasks/attachments.js'
import { TaskEditError, TaskFileChangedError, TaskNotFoundError } from '../tasks/task-inputs.js'
import { TemplateNotFoundError } from '../tasks/template-service.js'
import { NoTerminalError, TerminalLimitError } from '../terminals/terminal-types.js'
import { FileNotChangedError, WorkspaceConflictError } from '../workspaces/workspace-errors.js'
import {
  FileChangedOnDiskError,
  FileTooLargeError,
  GitFolderError,
  WorkspacePathError,
  WorkspacePathNotFoundError,
} from '../workspaces/workspace-files.js'
import { AgentRefusedError } from './auth.js'

function clientErrorStatus(error: unknown): number | undefined {
  if (
    typeof error === 'object' &&
    error !== null &&
    'statusCode' in error &&
    typeof error.statusCode === 'number' &&
    error.statusCode >= 400 &&
    error.statusCode < 500
  ) {
    return error.statusCode
  }
  return undefined
}

// One line a client can show as it is, such as "title: Too big: expected string to have <=400
// characters". The issues stay in the answer too, for the CLI.
function describeIssues(issues: z.ZodError['issues']): string {
  return issues
    .map(({ path, message }) =>
      path.length === 0 ? message : `${path.map(String).join('.')}: ${message}`,
    )
    .join('; ')
}

// The answer for each known error, checked in order: [class, status, code].
const ERROR_ANSWERS: [new (...args: never[]) => Error, number, string][] = [
  [TaskEditError, 400, 'invalid_request'],
  [AgentRefusedError, 403, 'forbidden'],
  [TaskNotFoundError, 404, 'not_found'],
  [UnknownAgentError, 404, 'unknown_agent'],
  [RepoInputError, 400, 'invalid_request'],
  [SettingsInputError, 400, 'invalid_request'],
  [StageFileError, 400, 'invalid_request'],
  [StageInputError, 400, 'invalid_request'],
  [AttachmentError, 400, 'invalid_request'],
  [TemplateNotFoundError, 404, 'not_found'],
  [RepoNotFoundError, 404, 'not_found'],
  [FileNotChangedError, 404, 'not_found'],
  [ImageSideMissingError, 404, 'not_found'],
  [WorkspacePathNotFoundError, 404, 'not_found'],
  [NotAnImageError, 400, 'invalid_request'],
  [WorkspacePathError, 400, 'invalid_request'],
  [ImageTooLargeError, 413, 'too_large'],
  [FileTooLargeError, 413, 'too_large'],
  [WorkspaceConflictError, 409, 'conflict'],
  [GitError, 409, 'conflict'],
  [OutsideWorkspaceError, 409, 'conflict'],
  [TerminalLimitError, 409, 'conflict'],
  [NoTerminalError, 409, 'conflict'],
  [NothingToCommitError, 409, 'conflict'],
  [GitHubError, 409, 'conflict'],
  [TaskWaitingError, 409, 'conflict'],
  [StageActionError, 409, 'conflict'],
  [IssueLinkError, 409, 'conflict'],
  [RepoInUseError, 409, 'conflict'],
  [SettingsChangedError, 409, 'conflict'],
  [TaskFileChangedError, 409, 'conflict'],
  [GitFolderError, 409, 'conflict'],
  [FileChangedOnDiskError, 409, 'conflict'],
]

export function errorHandler(error: unknown, request: FastifyRequest, reply: FastifyReply) {
  if (error instanceof z.ZodError) {
    return reply.code(400).send({
      error: 'invalid_request',
      issues: error.issues,
      message: describeIssues(error.issues),
    })
  }
  for (const [errorClass, status, code] of ERROR_ANSWERS) {
    if (error instanceof errorClass) {
      return reply.code(status).send({ error: code, message: error.message })
    }
  }
  const status = clientErrorStatus(error)
  if (status !== undefined) {
    const message = error instanceof Error ? error.message : 'Bad request'
    return reply.code(status).send({ error: 'bad_request', message })
  }
  request.log.error(error)
  return reply.code(500).send({ error: 'internal_error' })
}
