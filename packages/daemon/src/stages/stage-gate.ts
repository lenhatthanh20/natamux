import {
  codeStageBefore,
  offersCi,
  plainText,
  type Stage,
  stageName,
  stagesOf,
  type Task,
  type TaskEvent,
  terminalOfStage,
} from '@natamux/core'
import { listAttachments, withAttachmentSection } from '../tasks/attachments.js'
import { type CiResult, ciBranchOf } from './ci-check.js'
import { MAX_AGENT_SEND_BACKS, StageActionError } from './stage-actions.js'
import { runStageChecks } from './stage-checks.js'
import { readStageFile, workingDir, writeStageFile } from './stage-files.js'
import type { StageRun } from './stage-run.js'

// The stages whose CI check started and has no result yet.
export function stagesWaitingForCi(events: readonly { event: TaskEvent }[]): string[] {
  const waits = new Set<string>()
  for (const { event } of events) {
    const stage = event.payload.stage
    if (typeof stage !== 'string') {
      continue
    }
    if (event.type === 'stage.ci-started') {
      waits.add(stage)
    } else if (['stage.started', 'stage.checked', 'stage.check-failed'].includes(event.type)) {
      waits.delete(stage)
    }
  }
  return [...waits]
}

// A CI check that could not run, such as gh missing, is a failure no agent can fix.
function ciError(error: unknown): CiResult {
  const reason = error instanceof Error ? error.message : String(error)
  return { passed: false, reasons: [reason], passes: [], final: true }
}

// The reason of a send back for a CI failure. `logs` go to the agent, after the blank line.
function ciReason(stage: Stage, reasons: string[], logs: string[]): string {
  // The stage card shows the lines before the first blank line.
  return [
    `CI failed on ${stageName(stage)}:`,
    ...reasons.map((each) => `- ${each}`),
    '',
    ...logs.map((log) => plainText(log).trimEnd()),
    'Read the failed log with `gh run view <run id> --log-failed`; the id ends the run url.',
  ].join('\n')
}

// The gate of a stage: the report that it is done, its local checks and its CI check, then the
// pass, the fail or the send back.
export class StageGate {
  constructor(private readonly run: StageRun) {}

  // A CI check that ran when the daemon stopped goes on: it pushes the work again and waits.
  resumeCiChecks(): void {
    for (const task of this.run.tasks.listTasks()) {
      if (task.state === 'done' || task.state === 'cancelled') {
        continue
      }
      for (const stageId of stagesWaitingForCi(this.run.tasks.listEvents(task.id))) {
        this.startCiRound(task.id, stageId, this.lastStart(task.id, stageId).seq, [])
      }
    }
  }

  // The report that a stage is done, then its checks.
  async reportAndCheck(
    taskId: string,
    stage: string,
    source: TaskEvent['source'],
    terminal?: string,
  ): Promise<void> {
    const reported = this.run.tasks.applyEvent(taskId, {
      source,
      type: 'stage.reported',
      payload: { stage },
    })
    if (!reported.accepted) {
      throw new StageActionError(`${taskId}: the ${stage} stage is not running`)
    }
    if (source === 'agent' && terminal !== undefined) {
      this.run.reported.set(`${taskId}/${terminal}`, stage)
    }
    await this.check(taskId, stage)
  }

  private lastStart(taskId: string, stage: string) {
    const events = this.run.tasks.listEvents(taskId)
    for (let index = events.length - 1; index >= 0; index -= 1) {
      const stored = events[index]
      if (stored?.event.type === 'stage.started' && stored.event.payload.stage === stage) {
        const failures = events
          .slice(index)
          .filter(
            (later) =>
              later.event.type === 'stage.check-failed' && later.event.payload.stage === stage,
          ).length
        const head = stored.event.payload.head
        return {
          seq: stored.seq,
          at: stored.event.at,
          head: typeof head === 'string' ? head : null,
          failures,
        }
      }
    }
    return { seq: -1, at: null, head: null, failures: 0 }
  }

  private async check(taskId: string, stageId: string): Promise<void> {
    const key = `${taskId}/${stageId}/${this.lastStart(taskId, stageId).seq}`
    if (this.run.checking.has(key)) {
      return
    }
    this.run.checking.add(key)
    try {
      await this.checkOnce(taskId, stageId)
    } finally {
      this.run.checking.delete(key)
    }
  }

  private async checkOnce(taskId: string, stageId: string): Promise<void> {
    const task = this.run.tasks.getTask(taskId)
    const step = this.run.stepOf(task)
    const stage = stagesOf(step).find((candidate) => candidate.id === stageId)
    if (stage === undefined) {
      return
    }
    if (stage.kind === 'context') {
      this.addAttachmentSection(task, stage.output ?? 'context.md')
    }
    const start = this.lastStart(taskId, stageId)
    const result = await this.run.jobs.run(
      { kind: 'stage-check', label: `Checking ${stageName(stage)}`, taskId },
      () => this.runChecks(task, stage, start),
    )
    // The stage started again while the checks ran: their result is about the round before.
    if (this.lastStart(taskId, stageId).seq !== start.seq) {
      return
    }
    if (result.passed && this.waitsForCi(task, stage)) {
      this.askCi(task, stageId, start.seq, result.passes)
      return
    }
    if (result.passed) {
      await this.applyChecked(taskId, stageId, start.seq, result.passes)
      return
    }
    this.failChecks(taskId, stageId, result.reasons)
  }

  // The local checks of the stage. Checks that cannot run fail.
  private runChecks(task: Task, stage: Stage, start: { at: string | null; head: string | null }) {
    const folder = workingDir(this.run.dataDir, task)
    return runStageChecks(
      {
        stage,
        folder,
        cwd: task.workspace ?? folder,
        isWorktree: task.workspace !== null,
        startedAt: start.at,
        startHead: start.head,
      },
      this.run.runCommand,
    ).catch((error: unknown) => ({
      passed: false,
      reasons: [error instanceof Error ? error.message : String(error)],
      passes: [],
    }))
  }

  // Starts the CI check of a stage whose local checks passed. A second report waits for the
  // check that runs.
  private askCi(task: Task, stageId: string, seq: number, passes: string[]): void {
    if (this.run.ciRounds.has(`${task.id}/${stageId}`)) {
      return
    }
    this.run.tasks.recordEvent(task.id, 'stage.ci-started', {
      stage: stageId,
      branch: ciBranchOf(task.branch ?? ''),
    })
    this.startCiRound(task.id, stageId, seq, passes)
  }

  // The conditions of a branch after the stage can take minutes, and the stage may start again
  // meanwhile: then the checks are about the round before.
  private async applyChecked(taskId: string, stageId: string, seq: number, passes: string[]) {
    const event = await this.run.withPath(taskId, {
      source: 'system',
      type: 'stage.checked',
      payload: { stage: stageId, passes },
    })
    if (this.lastStart(taskId, stageId).seq === seq) {
      this.run.tasks.applyEvent(taskId, event)
    }
  }

  // The task asked for CI on this stage, and has the branch to push.
  private waitsForCi(task: Task, stage: Stage): boolean {
    const chosen = (task.ciStages ?? []).includes(stage.id)
    return chosen && offersCi(stage) && task.workspace !== null && task.branch !== null
  }

  private startCiRound(taskId: string, stageId: string, seq: number, passes: string[]): void {
    const key = `${taskId}/${stageId}`
    if (this.run.ciRounds.has(key)) {
      return
    }
    this.run.ciRounds.add(key)
    const task = this.run.tasks.getTask(taskId)
    const stage = this.run.tasks
      .stepsOf(task)
      .flatMap(stagesOf)
      .find((candidate) => candidate.id === stageId)
    const round = this.run.jobs.run(
      { kind: 'ci', label: `Waiting for CI on ${stageName(stage ?? { id: stageId })}`, taskId },
      () => this.ciRound(taskId, stageId, seq, passes),
    )
    this.run.background(round.finally(() => this.run.ciRounds.delete(key)))
  }

  // CI takes minutes, so it runs after the report has its answer. Its result counts only for the
  // round of the stage that asked for it.
  private async ciRound(taskId: string, stageId: string, seq: number, passes: string[]) {
    const { workspace, branch } = this.run.tasks.getTask(taskId)
    // The workspace can be removed while CI waits, then the daemon restarts. An empty worktree
    // would run git in the daemon's own folder and push that checkout.
    const ci =
      workspace === null || branch === null
        ? ciError('The task has no workspace or branch for the CI check.')
        : await this.run.checkCi({ worktree: workspace, branch }).catch(ciError)
    if (this.lastStart(taskId, stageId).seq !== seq) {
      return
    }
    if (ci.passed) {
      await this.applyChecked(taskId, stageId, seq, [...passes, ...ci.passes])
      return
    }
    if (!ci.final && this.sendBackForCi(taskId, stageId, ci.reasons, ci.logs)) {
      return
    }
    this.failChecks(taskId, stageId, ci.reasons, ci.final, ci.logs)
  }

  // A CI failure goes back to an agent as a send back, at most MAX_AGENT_SEND_BACKS times, so the
  // stage starts a new round and its prompt is pasted as a review's would be. An agent stage gets
  // it with the log. A testing stage only runs the code, so its failure goes to the stage that
  // changes code, without the log.
  private sendBackForCi(
    taskId: string,
    stageId: string,
    reasons: string[],
    logs = reasons,
  ): boolean {
    const task = this.run.tasks.getTask(taskId)
    const target = this.ciTarget(task, stageId)
    if (target === undefined) {
      return false
    }
    if (this.run.actions.sendBacksFrom(taskId, stageId) >= MAX_AGENT_SEND_BACKS) {
      this.failChecks(taskId, stageId, reasons, true)
      return true
    }
    // Ends the CI wait of the stage, so a restart does not check it again.
    this.run.tasks.applyEvent(taskId, {
      source: 'system',
      type: 'stage.check-failed',
      payload: { stage: stageId, reasons, final: false },
    })
    const { stage, to } = target
    const reason = ciReason(stage, reasons, stage.kind === 'agent' ? logs : [])
    this.reopenForCi(task, to.id, stageId, reason)
    return true
  }

  // The stage a CI failure goes back to: an agent stage itself, a testing stage the stage before
  // it that changes code. None for other stages.
  private ciTarget(task: Task, stageId: string): { stage: Stage; to: Stage } | undefined {
    const steps = this.run.tasks.stepsOf(task)
    const stage = stagesOf(this.run.stepOf(task)).find((candidate) => candidate.id === stageId)
    const own = stage?.kind === 'agent'
    const to = own
      ? stage
      : stage?.kind === 'testing'
        ? codeStageBefore(steps, stageId, task.paths)
        : undefined
    if (stage === undefined || to === undefined) {
      return undefined
    }
    return { stage, to }
  }

  private reopenForCi(task: Task, to: string, stageId: string, reason: string): void {
    this.run.leaving.set(task.id, this.run.stepOf(task))
    try {
      this.run.tasks.applyEvent(task.id, {
        source: 'system',
        type: 'stage.reopened',
        payload: { to, reason, from: stageId, byAgent: true, ci: true },
      })
    } finally {
      this.run.leaving.delete(task.id)
    }
  }

  // An agent gets one more try; a stage that no agent works on, or a failure no agent can fix,
  // waits for you at once.
  private failChecks(
    taskId: string,
    stageId: string,
    reasons: string[],
    cannotFix = false,
    // What the agent reads, when it is longer than the reasons the stage card shows.
    logs = reasons,
  ) {
    const task = this.run.tasks.getTask(taskId)
    const step = this.run.stepOf(task)
    const stage = stagesOf(step).find((candidate) => candidate.id === stageId)
    if (stage === undefined) {
      return
    }
    const start = this.lastStart(taskId, stageId)
    const terminal = terminalOfStage(this.run.tasks.stepsOf(task), stageId)
    const agentStage =
      stage.kind === 'agent' || stage.kind === 'testing' || this.run.context.isAgentContext(task)
    const agentWorks = agentStage && this.run.shells.agentRuns(taskId, terminal)
    const final = cannotFix || !agentWorks || start.failures >= 1
    this.run.tasks.applyEvent(taskId, {
      source: 'system',
      type: 'stage.check-failed',
      payload: { stage: stageId, reasons, final },
    })
    if (!final) {
      const text = [
        'The checks of this stage failed:',
        ...logs.map((reason) => `- ${reason}`),
        'Fix them, then run `natamux done` again.',
      ].join('\n')
      try {
        this.run.terminals.paste(taskId, text, terminal)
      } catch {
        // The terminal closed since: the reasons are in the log, and the next failure waits.
      }
    }
  }

  // An agent that read the context from a skill or an MCP server may leave the attachments out of
  // it. The later stages find them only through the context.
  private addAttachmentSection(task: Task, output: string): void {
    const attachments = listAttachments(this.run.dataDir, task.id)
    const text =
      attachments.length === 0 ? undefined : readStageFile(this.run.dataDir, task, output)
    if (text === undefined) {
      return
    }
    const next = withAttachmentSection(text, attachments)
    if (next !== text) {
      writeStageFile(this.run.dataDir, task, output, next)
    }
  }
}
