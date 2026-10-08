// iOS PWA full screen (adapted from tabubu). Installed, iOS reports the viewport too short: the safe
// area is wrongly subtracted and 100vh/100dvh/innerHeight all return that value. Without a browser bar
// the screen height is the truth. Safe-area values are not ready on a cold start, so measure again in
// steps. iOS only (navigator.standalone): on Android screen.height exceeds the visible area and the
// correction would cut off buttons.
// Sets --app-height and calls onFit(sizeChanged) after every measurement.
export function watchViewport(onFit) {
  let width = 0, height = 0;
  const fit = () => {
    let h = window.visualViewport?.height ?? innerHeight;
    if (navigator.standalone === true) {
      const screenHeight = innerHeight > innerWidth ? Math.max(screen.height, screen.width) : Math.min(screen.height, screen.width);
      if (screenHeight - h > 15) h = screenHeight; // smaller differences are rounding
    }
    document.documentElement.style.setProperty('--app-height', `${Math.round(h)}px`);
    const changed = width !== innerWidth || height !== h;
    width = innerWidth;
    height = h;
    onFit(changed);
  };
  addEventListener('resize', fit);
  window.visualViewport?.addEventListener('resize', fit);
  addEventListener('orientationchange', () => { setTimeout(fit, 100); setTimeout(fit, 300); });
  // iOS discards background PWAs and restarts them; measure again on return.
  document.addEventListener('visibilitychange', () => { if (!document.hidden) setTimeout(fit, 50); });
  fit();
  for (const ms of [50, 150, 300, 500, 800, 1200]) setTimeout(fit, ms);
}
