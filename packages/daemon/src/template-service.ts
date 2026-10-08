import { mkdirSync, readdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { type Task, type TaskTemplate, taskTemplateSchema } from '@helmsman/core'
import matter from 'gray-matter'
import { z } from 'zod'
import type { TaskService } from './task-service.js'
import { slugify } from './workspace-service.js'

const TEMPLATE_FILE_NAME = /^([a-z0-9-]+)\.md$/

export const newTemplateInputSchema = taskTemplateSchema.omit({ id: true })
export const fromTemplateInputSchema = z.object({ title: z.string().trim().min(1).max(200) })

export class TemplateNotFoundError extends Error {
  constructor(id: string) {
    super(`Template ${id} does not exist`)
    this.name = 'TemplateNotFoundError'
  }
}

function isMissingFile(error: unknown): boolean {
  return error instanceof Error && 'code' in error && error.code === 'ENOENT'
}

// Templates live in <data dir>/templates, one markdown file each: the fields in the front matter,
// the notes of the new task as the body.
export class TemplateService {
  private readonly dir: string
  private readonly tasks: TaskService

  constructor(options: { dataDir: string; tasks: TaskService }) {
    this.dir = join(options.dataDir, 'templates')
    this.tasks = options.tasks
  }

  list(): TaskTemplate[] {
    let names: string[]
    try {
      names = readdirSync(this.dir)
    } catch (error) {
      if (isMissingFile(error)) {
        return []
      }
      throw error
    }
    return names
      .flatMap((name) => {
        const id = TEMPLATE_FILE_NAME.exec(name)?.[1]
        return id ? [this.get(id)] : []
      })
      .sort((a, b) => a.name.localeCompare(b.name))
  }

  get(id: string): TaskTemplate {
    let text: string
    try {
      text = readFileSync(join(this.dir, `${id}.md`), 'utf8')
    } catch (error) {
      if (isMissingFile(error)) {
        throw new TemplateNotFoundError(id)
      }
      throw error
    }
    const { data, content } = matter(text)
    return taskTemplateSchema.parse({ ...data, id, description: content.trim() })
  }

  // Saves a template under an id made from its name. A template with the same name is replaced.
  save(rawInput: unknown): TaskTemplate {
    const input = newTemplateInputSchema.parse(rawInput)
    const template = taskTemplateSchema.parse({ ...input, id: slugify(input.name) })
    const { id, description, ...fields } = template
    mkdirSync(this.dir, { recursive: true })
    const path = join(this.dir, `${id}.md`)
    writeFileSync(`${path}.tmp`, matter.stringify(`${description}\n`, fields))
    renameSync(`${path}.tmp`, path)
    return template
  }

  // A new draft task with the template's notes, priority and labels, and the given title.
  createTask(id: string, rawInput: unknown): Task {
    const template = this.get(id)
    const { title } = fromTemplateInputSchema.parse(rawInput)
    return this.tasks.createTask({
      title,
      description: template.description,
      priority: template.priority,
      labels: template.labels,
    })
  }
}
