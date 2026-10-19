import type { ContextSource, Stage, Step, Task } from '@natamux/core'
import { resolveSkill } from '../agents/agent-skills.js'
import { type GhIssue, issueContext, parseIssueRef } from '../github/issue-ref.js'
import { listAttachments, withAttachmentSection } from '../tasks/attachments.js'
import { StageActionError } from './stage-actions.js'
import { writeStageFile } from './stage-files.js'
import { contextPrompt, manualContext } from './stage-prompts.js'
import type { StageRun } from './stage-run.js'

// A source that an agent reads the context from.
type AgentSource = Extract<ContextSource, { type: 'skill' | 'mcp' }>

// The context stage: reads the task's context from its source into the stage file.
export class StageContext {
  constructor(private readonly run: StageRun) {}

  private sourceOf(task: Task): ContextSource {
    const type = task.context?.source ?? 'manual'
    const skill = task.context?.skill
    if (type === 'skill' && skill) {
      return { type: 'skill', name: skill, skill }
    }
    const name = task.context?.name
    const offered = this.run.tasks
      .settingsOf(task.repo)
      .context.sources.filter((source) => source.type === type)
    // A task from before names were sent, or one whose source was renamed, takes the first.
    const configured =
      offered.find((source) => 'name' in source && source.name === name) ?? offered[0]
    return configured ?? (type === 'skill' || type === 'mcp' ? { type: 'manual' } : { type })
  }

  async startContext(task: Task, step: Step, stage: Stage): Promise<void> {
    const source = this.sourceOf(task)
    if (source.type === 'skill' || source.type === 'mcp') {
      await this.startContextAgent(task, step, stage, source)
      return
    }
    await this.fetchContext(task, stage, source)
  }

  // An agent reads the context with the skill or the MCP server of the source.
  private async startContextAgent(
    task: Task,
    step: Step,
    stage: Stage,
    source: AgentSource,
  ): Promise<void> {
    const attachments = listAttachments(this.run.dataDir, task.id)
    await this.run.launcher.startAgent(task, step, stage, (agent) => {
      const skill =
        source.type === 'skill'
          ? resolveSkill(source.skill, this.run.launcher.skillContext(task, agent))
          : undefined
      return {
        text: contextPrompt(source, task, attachments, skill),
        missing: source.type === 'skill' && skill?.kind === 'missing' ? [source.skill] : [],
      }
    })
  }

  // The daemon writes the context of the issue, or of the task itself, then checks it.
  private async fetchContext(task: Task, stage: Stage, source: ContextSource): Promise<void> {
    if (!this.run.launcher.markStarted(task, stage, { source: source.type })) {
      return
    }
    const output = stage.output ?? 'context.md'
    try {
      const text = source.type === 'github-issue' ? await this.readIssue(task) : manualContext(task)
      const attachments = listAttachments(this.run.dataDir, task.id)
      // The task may have got its workspace while the issue was read: write where it is now.
      writeStageFile(
        this.run.dataDir,
        this.run.tasks.getTask(task.id),
        output,
        withAttachmentSection(text, attachments),
      )
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error)
      this.run.launcher.failStage(task.id, stage.id, reason)
      return
    }
    this.run.tasks.recordEvent(task.id, 'context.fetched', { stage: stage.id, source: source.type })
    await this.run.gate.reportAndCheck(task.id, stage.id, 'system')
    // A context of the task's own title and description holds nothing new to read. Waiting for
    // approval would put every new backlog task in Needs you.
    const now = this.run.tasks.getTask(task.id)
    if (source.type === 'manual' && now.stage === stage.id && now.state === 'review') {
      await this.run.applyEnding(task.id, {
        source: 'system',
        type: 'stage.passed',
        payload: { stage: stage.id },
      })
    }
  }

  private async readIssue(task: Task): Promise<string> {
    const ref =
      parseIssueRef(task.context?.ref ?? '') ??
      (task.issue?.provider === 'github' ? parseIssueRef(task.issue.key) : undefined)
    if (ref === undefined) {
      throw new StageActionError('Give the issue number, such as #42, to read the context from')
    }
    // An address of another repository reads that repository's issue, not the same number here.
    const output = await this.run.gh(this.run.repoPath(task.repo), [
      'issue',
      'view',
      String(ref.number),
      ...(ref.repo === undefined ? [] : ['--repo', ref.repo]),
      '--json',
      'number,title,body,url,labels,comments',
    ])
    const issue = JSON.parse(output) as GhIssue
    if (this.run.tasks.getTask(task.id).issue === null && issue.url) {
      const number = issue.number ?? ref.number
      this.run.tasks.linkIssue(
        task.id,
        {
          provider: 'github',
          key: ref.slug === undefined ? `#${number}` : `${ref.slug}#${number}`,
          url: issue.url,
        },
        'system',
      )
    }
    return issueContext(issue)
  }

  isAgentContext(task: Task): boolean {
    const type = this.sourceOf(task).type
    return type === 'skill' || type === 'mcp'
  }
}
