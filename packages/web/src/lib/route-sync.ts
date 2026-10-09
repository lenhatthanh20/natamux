import { isSamePage, parseRoute, routeOf, routePath } from './route'
import { useViewStore } from './view-store'

// Keeps the address bar and the view store in step, in both directions. Call once at start,
// before the first render.
export function startRouting(): void {
  const apply = () => {
    useViewStore.getState().applyRoute(parseRoute(location.pathname, location.search))
  }
  apply()
  history.replaceState(null, '', routePath(routeOf(useViewStore.getState())))

  useViewStore.subscribe((state) => {
    const next = routePath(routeOf(state))
    const here = `${location.pathname}${location.search}`
    if (next === here) {
      return
    }
    if (isSamePage(next, here)) {
      history.replaceState(null, '', next)
    } else {
      history.pushState(null, '', next)
    }
  })

  // The back and forward buttons.
  window.addEventListener('popstate', apply)
}
