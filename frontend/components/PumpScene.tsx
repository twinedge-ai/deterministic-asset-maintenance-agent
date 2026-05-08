"use client";

import type { CSSProperties } from "react";
import type { RiskLevel } from "../lib/api";

type PumpSceneProps = {
  riskLevel: RiskLevel;
  npshMarginM: number | null;
  flowRateM3h: number | null;
  cavitationScore: number;
  modeLabel: string;
  windowLabel: string;
  measurements: PumpMeasurement[];
};

export type PumpMeasurement = {
  label: string;
  value: string;
  tone?: "normal" | "risk" | "muted";
};

export function PumpScene({
  riskLevel,
  npshMarginM,
  flowRateM3h,
  cavitationScore,
  modeLabel,
  windowLabel,
  measurements,
}: PumpSceneProps) {
  const flowSpeed = Math.min(Math.max((flowRateM3h ?? 900) / 1200, 0.55), 1.25);
  const bubbleCount = Math.max(3, Math.round(cavitationScore * 14));
  const stageCount = findMeasurement(measurements, "Stages");
  const ratedDuty = `${stageCount}-stage / ${findMeasurement(measurements, "Rated flow")} / ${findMeasurement(
    measurements,
    "Rated head",
  )}`;
  const npshr = findMeasurement(measurements, "NPSHR");

  return (
    <div
      className={`pump-schematic pump-schematic-${riskLevel.toLowerCase()}`}
      data-testid="pump-schematic"
      style={{ "--flow-speed": `${2.4 / flowSpeed}s` } as CSSProperties}
    >
      <div className="schematic-header">
        <div>
          <span>Live data flow</span>
          <strong>{modeLabel}</strong>
        </div>
        <div>
          <span>Analysis window</span>
          <strong>{windowLabel}</strong>
        </div>
        <div>
          <span>Rated duty</span>
          <strong>{ratedDuty}</strong>
        </div>
      </div>

      <div className="schematic-board" aria-label="BB3 multistage centrifugal pump measurement schematic">
        <svg className="pump-diagram" viewBox="0 0 1000 420" role="img" aria-hidden="true">
          <defs>
            <marker id="flow-arrow" markerHeight="8" markerWidth="10" orient="auto" refX="9" refY="4">
              <path d="M0,0 L10,4 L0,8 Z" />
            </marker>
            <linearGradient id="case-gradient" x1="0" x2="0" y1="0" y2="1">
              <stop offset="0%" stopColor="#ffffff" />
              <stop offset="100%" stopColor="#dfe6e4" />
            </linearGradient>
          </defs>

          <path className="pipe-shell" d="M36 208 H244" />
          <path className="flow-line flow-line-suction" d="M36 208 H244" />
          <path className="pipe-shell discharge-riser" d="M640 208 C676 208 684 160 714 160 H958" />
          <path className="flow-line flow-line-discharge" d="M640 208 C676 208 684 160 714 160 H958" />

          <g className="pipe-label suction-label">
            <rect height="24" rx="12" width="120" x="78" y="162" />
            <text x="138" y="178">Suction</text>
          </g>
          <g className="pipe-label discharge-label">
            <rect height="24" rx="12" width="132" x="770" y="116" />
            <text x="836" y="132">Discharge</text>
          </g>

          <rect className="baseplate" height="26" rx="4" width="720" x="196" y="342" />
          <rect className="baseplate-rail" height="10" rx="3" width="760" x="176" y="370" />

          <rect className="bearing pedestal-left" height="70" rx="5" width="50" x="218" y="178" />
          <rect className="bearing pedestal-right" height="70" rx="5" width="50" x="642" y="178" />
          <rect className="seal seal-left" height="50" rx="4" width="18" x="278" y="188" />
          <rect className="seal seal-right" height="50" rx="4" width="18" x="614" y="188" />

          <path
            className="split-casing"
            d="M286 214 C286 140 350 114 466 114 H528 C612 114 640 158 640 214 C640 270 612 304 528 304 H466 C350 304 286 286 286 214 Z"
          />
          <path
            className="casing-top"
            d="M292 204 C302 144 360 124 466 124 H528 C594 124 628 154 634 204 Z"
          />
          <path className="parting-line" d="M292 210 H634" />
          <path className="lower-nozzle suction-nozzle" d="M244 208 H306" />
          <path className="lower-nozzle discharge-nozzle" d="M620 208 H650" />

          <path className="shaft" d="M230 210 H760" />
          <g className="stage-group">
            {[338, 424, 510, 596].map((x, index) => (
              <g className={index < 2 ? "stage stage-forward" : "stage stage-opposed"} key={x}>
                <circle cx={x} cy="210" r="36" />
                <path
                  d={`M${x - 18} 210 C${x - 4} 178 ${x + 24} 178 ${x + 18} 210 C${x + 4} 242 ${
                    x - 24
                  } 242 ${x - 18} 210 Z`}
                />
                <path d={`M${x} 174 L${x} 246`} />
                <text x={x} y="262">{index + 1}</text>
              </g>
            ))}
          </g>
          <path className="interstage-channel upper-channel" d="M374 158 C392 144 404 144 424 158" />
          <path className="interstage-channel lower-channel" d="M460 262 C478 278 492 278 510 262" />
          <path className="interstage-channel upper-channel" d="M546 158 C564 144 578 144 596 158" />

          <g className="case-label">
            <text x="462" y="102">Axially split BB3 casing</text>
          </g>
          <g className="npshr-tag">
            <rect height="24" rx="12" width="114" x="304" y="132" />
            <text x="361" y="148">NPSHR {npshr}</text>
          </g>

          <rect className="coupling" height="30" rx="15" width="58" x="710" y="195" />
          <rect className="motor-body" height="92" rx="10" width="150" x="780" y="164" />
          <rect className="motor-fin" height="76" width="10" x="802" y="172" />
          <rect className="motor-fin" height="76" width="10" x="824" y="172" />
          <rect className="motor-fin" height="76" width="10" x="846" y="172" />
          <rect className="motor-foot" height="20" rx="3" width="110" x="800" y="256" />
          <text className="motor-text" x="855" y="215">Motor</text>

          <g className="cavitation-zone">
            {Array.from({ length: bubbleCount }, (_, index) => {
              const column = index % 5;
              const row = Math.floor(index / 5) % 3;
              return (
                <circle
                  cx={310 + column * 12}
                  cy={196 + row * 14}
                  key={index}
                  r="4"
                  style={{ "--bubble-index": index } as CSSProperties}
                />
              );
            })}
          </g>
        </svg>

        <Callout className="callout-flow" label="Flow" value={findMeasurement(measurements, "Flow")} />
        <Callout className="callout-suction" label="Suction pressure" value={findMeasurement(measurements, "Suction")} tone="risk" />
        <Callout className="callout-temp" label="Liquid temp" value={findMeasurement(measurements, "Liquid temp")} />
        <Callout className="callout-npsh" label="NPSH margin" value={formatSigned(npshMarginM, "m")} tone="risk" />
        <Callout className="callout-vibration" label="Vibration" value={findMeasurement(measurements, "Vibration")} tone="risk" />
        <Callout className="callout-bearing" label="Bearing temp" value={findMeasurement(measurements, "Bearing temp")} />
        <Callout className="callout-discharge" label="Discharge pressure" value={findMeasurement(measurements, "Discharge")} />
        <Callout className="callout-current" label="Motor current" value={findMeasurement(measurements, "Current")} />
        <Callout className="callout-speed" label="Speed" value={findMeasurement(measurements, "Speed")} />
      </div>

      <div className="schematic-data-grid">
        {measurements.map((measurement) => (
          <div className={`schematic-data-card data-${measurement.tone ?? "normal"}`} key={measurement.label}>
            <span>{measurement.label}</span>
            <strong>{measurement.value}</strong>
          </div>
        ))}
      </div>
    </div>
  );
}

function Callout({
  className,
  label,
  value,
  tone = "normal",
}: {
  className: string;
  label: string;
  value: string;
  tone?: PumpMeasurement["tone"];
}) {
  return (
    <div className={`schematic-callout ${className} callout-${tone ?? "normal"}`}>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function findMeasurement(measurements: PumpMeasurement[], label: string) {
  return measurements.find((measurement) => measurement.label === label)?.value ?? "UNKNOWN";
}

function formatSigned(value: number | null, unit: string) {
  if (value === null || !Number.isFinite(value)) {
    return "UNKNOWN";
  }
  const sign = value > 0 ? "+" : "";
  return `${sign}${value.toFixed(2)} ${unit}`;
}
