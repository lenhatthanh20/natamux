// A path that may wrap after a slash, never in the middle of a folder name.
export function PathText({ path }: { path: string }) {
  const parts = path.split('/').map((part, index, all) => ({
    key: all.slice(0, index + 1).join('/'),
    part,
    first: index === 0,
  }))

  return (
    <span className="font-mono" title={path}>
      {parts.map(({ key, part, first }) => (
        <span key={key}>
          {!first && (
            <>
              /<wbr />
            </>
          )}
          {part}
        </span>
      ))}
    </span>
  )
}
