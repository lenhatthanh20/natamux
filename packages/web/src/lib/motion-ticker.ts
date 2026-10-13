// An endless CSS animation makes the browser draw every frame: 120 a second on a 120 Hz screen.
// This pauses every endless CSS animation and moves them all to the same time a few times a
// second, so the page draws only then, however many elements move. CSS steps() did not help once
// elements started at different times. Transitions and one-shot animations are not touched.
export const MOTION_TICK_MS = 125

export function startMotionTicker(target: Document = document): () => void {
  if (typeof target.getAnimations !== 'function') {
    return () => {}
  }
  const tick = () => {
    // A hidden page draws nothing. The next tick after it shows moves everything again.
    if (target.visibilityState === 'hidden') {
      return
    }
    const now = Math.floor(performance.now() / MOTION_TICK_MS) * MOTION_TICK_MS
    for (const animation of target.getAnimations()) {
      // A CSS transition has no animationName.
      if (
        !('animationName' in animation) ||
        animation.effect?.getTiming().iterations !== Number.POSITIVE_INFINITY
      ) {
        continue
      }
      if (animation.playState !== 'paused') {
        animation.pause()
      }
      animation.currentTime = now
    }
  }
  tick()
  const timer = setInterval(tick, MOTION_TICK_MS)
  return () => clearInterval(timer)
}
