import { useNowValue } from '../../hooks/useNow'
import { collapsedHint, type RepoGroup } from '../../lib/workspace-groups'

// The dot of a folded group. It reads the clock itself: a workspace in it can turn quiet.
export function HiddenDot({
  group,
  currentId,
  repoName,
}: {
  group: RepoGroup
  currentId: string | null
  repoName: string
}) {
  const hint = useNowValue((now) => collapsedHint(group, currentId, now))
  return hint === null ? null : (
    <span
      className="state-dot"
      style={{ width: 6, height: 6 }}
      data-state={hint}
      data-tip={`A workspace in ${repoName} is hidden here`}
      aria-hidden="true"
    />
  )
}
