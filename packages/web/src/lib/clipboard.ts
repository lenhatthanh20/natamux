import { toast } from './toasts'

// Copies text and says so in a toast. `what` is for the message: "the branch name".
export function copyText(text: string, what: string): void {
  navigator.clipboard
    ?.writeText(text)
    .then(() => toast(`Copied ${what}.`))
    .catch(() => toast(`Could not copy ${what}.`, 'error'))
}
