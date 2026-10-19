import { useEffect } from 'react'

// Tells the desktop app how many tasks need the user and how many run: the count shows on the app
// icon, and running agents keep the computer awake. A browser tab has no app to tell.
export function useDesktopStatus(needsYou: number, running: number): void {
  useEffect(() => {
    window.natamux?.reportStatus?.({ needsYou, running })
  }, [needsYou, running])
}
