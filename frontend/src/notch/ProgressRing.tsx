import type { ReactNode } from "react";

type ProgressRingProps = {
  value: number | null;
  label: string;
  semantic?: "progress" | "activity";
  children: ReactNode;
  className?: string;
};

const RADIUS = 22;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

export function ProgressRing({
  value,
  label,
  semantic = "progress",
  children,
  className = "",
}: ProgressRingProps) {
  const normalized = value === null ? null : Math.min(100, Math.max(0, value));
  const offset = normalized === null ? CIRCUMFERENCE : CIRCUMFERENCE * (1 - normalized / 100);
  const accessibility = semantic === "progress" && normalized !== null
    ? { role: "progressbar", "aria-label": label, "aria-valuemin": 0, "aria-valuemax": 100, "aria-valuenow": normalized }
    : { role: "group", "aria-label": label };

  return <span className={`progress-ring ${normalized === null ? "is-neutral" : ""} ${className}`.trim()} {...accessibility}>
    <svg viewBox="0 0 52 52" aria-hidden="true">
      <circle className="progress-ring__track" cx="26" cy="26" r={RADIUS} />
      <circle
        className="progress-ring__value"
        cx="26"
        cy="26"
        r={RADIUS}
        pathLength={CIRCUMFERENCE}
        strokeDasharray={CIRCUMFERENCE}
        strokeDashoffset={offset}
        data-ring-value={normalized ?? undefined}
      />
    </svg>
    <span className="progress-ring__center">{children}</span>
  </span>;
}
