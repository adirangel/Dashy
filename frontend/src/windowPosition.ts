import { getCurrentWindow, PhysicalPosition, primaryMonitor } from "@tauri-apps/api/window";

type Position = { x: number; y: number };
type Size = { width: number; height: number };

type TopRightPositionInput = {
  workArea: { position: Position; size: Size };
  windowSize: Size;
  scaleFactor: number;
  margin: number;
};

export function topRightPosition({ workArea, windowSize, scaleFactor, margin }: TopRightPositionInput): Position {
  const marginInPixels = Math.round(margin * scaleFactor);
  const minimumX = workArea.position.x + marginInPixels;
  const minimumY = workArea.position.y + marginInPixels;

  return {
    x: Math.max(minimumX, workArea.position.x + workArea.size.width - windowSize.width - marginInPixels),
    y: minimumY,
  };
}

export async function positionDashyWindow(margin = 18): Promise<void> {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;

  const monitor = await primaryMonitor();
  if (!monitor) return;

  const appWindow = getCurrentWindow();
  const windowSize = await appWindow.outerSize();
  const position = topRightPosition({
    workArea: monitor.workArea,
    windowSize,
    scaleFactor: monitor.scaleFactor,
    margin,
  });

  await appWindow.setPosition(new PhysicalPosition(position.x, position.y));
}
