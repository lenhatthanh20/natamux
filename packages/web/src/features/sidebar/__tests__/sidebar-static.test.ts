import { describe, expect, it } from 'vitest'
import badge from '../../../components/ui/CountBadge.tsx?raw'
import clearDialog from '../ClearFinishedDialog.tsx?raw'
import hiddenDot from '../HiddenDot.tsx?raw'
import repoHeader from '../RepoHeader.tsx?raw'
import sidebar from '../Sidebar.tsx?raw'
import list from '../WorkspaceList.tsx?raw'
import row from '../WorkspaceRow.tsx?raw'

const FILES = { list, row, sidebar, badge, hiddenDot, clearDialog, repoHeader }

// The sidebar takes its colors from the tokens and its icons from lucide-react (docs/ui-spec.md).
describe.each(Object.entries(FILES))('%s', (_name, source) => {
  it('has no color value of its own', () => {
    expect(source.match(/#[0-9a-fA-F]{3,8}\b/g)).toBeNull()
  })

  it('takes no icon or animation package but lucide-react', () => {
    const packages = [...source.matchAll(/from '([^'.][^']*)'/g)].map((match) => match[1])
    expect(packages.filter((name) => /icon|motion|animat|spring/i.test(name ?? ''))).toEqual([])
  })
})
