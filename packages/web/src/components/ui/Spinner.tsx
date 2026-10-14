// Eight dashes: a bright head and a tail that fades behind it. It turns one dash per 125 ms tick
// of lib/motion-ticker.ts (.hm-spin8 in styles.css), so it looks the same at 8 and 60 frames a
// second. Still, with reduced motion or a window without focus, it shows three dots.
const OPACITY = [1, 0.3, 0.3, 0.3, 0.36, 0.48, 0.64, 0.82]
const RADIUS = 8
const HALF_DASH = (8 * Math.PI) / 180

const point = (angle: number) =>
  `${(12 + RADIUS * Math.cos(angle)).toFixed(2)} ${(12 + RADIUS * Math.sin(angle)).toFixed(2)}`

const DASHES = OPACITY.map((opacity, index) => {
  const angle = ((index * 45 - 90) * Math.PI) / 180
  const from = point(angle - HALF_DASH)
  const to = point(angle + HALF_DASH)
  return { d: `M${from}A${RADIUS} ${RADIUS} 0 0 1 ${to}`, opacity }
})

export function Spinner({ size }: { size: number }) {
  return (
    <svg className="hm-spin8" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
      <g
        className="hm-spin-dashes"
        fill="none"
        stroke="currentColor"
        strokeWidth={size <= 12 ? 3 : 2.6}
        strokeLinecap="round"
      >
        {DASHES.map(({ d, opacity }) => (
          <path key={d} d={d} opacity={opacity} />
        ))}
      </g>
      <g className="hm-spin-dots" fill="currentColor">
        <circle cx={5} cy={12} r={2.1} />
        <circle cx={12} cy={12} r={2.1} />
        <circle cx={19} cy={12} r={2.1} />
      </g>
    </svg>
  )
}
