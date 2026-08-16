import * as React from "react"

/**
 * A page saying it is a work surface rather than something to read.
 *
 * Every dashboard page is centred at 1024 pixels, which is right for reading:
 * a paragraph the width of a monitor is a paragraph nobody finishes. It is
 * wrong for a canvas. Measured on the flows editor — 1024 less its padding,
 * less the gap, less a 320-pixel inspector left the canvas 640 pixels, and a
 * step is 208 wide, so three fitted side by side on a 2560-pixel screen
 * exactly as on a 1024-pixel one. The palette had already been moved out of
 * the way; the page around it was doing the squeezing.
 *
 * So a page can ask for the width it was given. It stays the exception —
 * asking for it is a line of code and reading it back is nothing.
 */
export const WideSurface = React.createContext<{
  wide: boolean
  setWide: (wide: boolean) => void
}>({ wide: false, setWide: () => undefined })

/** Whether the page on screen has asked for the whole width. */
export function useWideSurface() {
  return React.useContext(WideSurface).wide
}

/**
 * Take the whole width while this is on screen, and give it back on the way
 * out — a page that forgot would leave every page after it uncentred.
 */
export function useAskForTheWidth(wanted: boolean) {
  const { setWide } = React.useContext(WideSurface)

  React.useEffect(() => {
    setWide(wanted)
    return () => setWide(false)
  }, [wanted, setWide])
}
