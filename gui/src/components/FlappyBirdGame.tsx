import { useCallback, useEffect, useRef, useState } from "react";
import useModalDialog from "../hooks/useModalDialog";
import {
  persistFlappyHighScore,
  readFlappyHighScore,
} from "../lib/flappyScore";

type CharacterSkin = "dario" | "sam";
type GameStatus = "ready" | "playing" | "over";

interface Props {
  onClose: () => void;
}

interface Pipe {
  x: number;
  gapY: number;
  counted: boolean;
}

interface World {
  birdY: number;
  velocity: number;
  pipes: Pipe[];
  spawnIn: number;
  lastTime: number;
  score: number;
  scroll: number;
}

const WIDTH = 480;
const HEIGHT = 360;
const GROUND_Y = 330;
const BIRD_X = 108;
const BIRD_RADIUS = 15;
const PIPE_WIDTH = 58;
const PIPE_GAP = 116;
const PIPE_SPEED = 145;
const PIPE_INTERVAL = 1.55;
const GRAVITY = 900;
const FLAP_VELOCITY = -330;

function newWorld(): World {
  return {
    birdY: HEIGHT / 2,
    velocity: FLAP_VELOCITY,
    pipes: [{ x: WIDTH + 30, gapY: 176, counted: false }],
    spawnIn: PIPE_INTERVAL,
    lastTime: 0,
    score: 0,
    scroll: 0,
  };
}

function drawPipe(context: CanvasRenderingContext2D, pipe: Pipe) {
  const gapTop = pipe.gapY - PIPE_GAP / 2;
  const gapBottom = pipe.gapY + PIPE_GAP / 2;

  context.fillStyle = "#185f4b";
  context.fillRect(pipe.x, 0, PIPE_WIDTH, gapTop);
  context.fillRect(pipe.x, gapBottom, PIPE_WIDTH, GROUND_Y - gapBottom);
  context.fillStyle = "#2e8b68";
  context.fillRect(pipe.x + 6, 0, 10, gapTop);
  context.fillRect(pipe.x + 6, gapBottom, 10, GROUND_Y - gapBottom);
  context.fillStyle = "#124638";
  context.fillRect(pipe.x - 5, gapTop - 18, PIPE_WIDTH + 10, 18);
  context.fillRect(pipe.x - 5, gapBottom, PIPE_WIDTH + 10, 18);
  context.fillStyle = "#45aa80";
  context.fillRect(pipe.x, gapTop - 15, PIPE_WIDTH, 5);
  context.fillRect(pipe.x, gapBottom + 3, PIPE_WIDTH, 5);
}

function PilotPortrait({ skin }: { skin: CharacterSkin }) {
  const isDario = skin === "dario";
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 120 120"
      className="h-28 w-28 drop-shadow-sm"
    >
      <defs>
        <radialGradient id={`face-${skin}`} cx="42%" cy="32%" r="72%">
          <stop offset="0%" stopColor={isDario ? "#f4d3b2" : "#f7d8ba"} />
          <stop offset="62%" stopColor={isDario ? "#e9c19c" : "#eec5a2"} />
          <stop offset="100%" stopColor={isDario ? "#d3a37c" : "#d9ab84"} />
        </radialGradient>
        <linearGradient id={`hair-${skin}`} x1="0.3" y1="0" x2="0.7" y2="1">
          <stop offset="0%" stopColor={isDario ? "#4a352c" : "#7a6249"} />
          <stop offset="100%" stopColor={isDario ? "#241a15" : "#4c3a2c"} />
        </linearGradient>
      </defs>

      <circle cx="60" cy="60" r="56" fill={isDario ? "#f3e6dd" : "#dcefeb"} />

      {/* Shoulders and neck are shared; clothing colour differs per pilot. */}
      <path d="M22 118c4-20 16-30 38-30s34 10 38 30z" fill={isDario ? "#2b3a4d" : "#8d959e"} />
      <path d="M51 70h18v20H51z" fill={isDario ? "#dcae88" : "#e1b58f"} />
      <path d="M51 70h18v7c-6 3-12 3-18 0z" fill="#c08f6b" opacity="0.45" />
      <ellipse cx="35" cy="53" rx="4.5" ry="7" fill={isDario ? "#e9c19c" : "#eec5a2"} />
      <ellipse cx="85" cy="53" rx="4.5" ry="7" fill={isDario ? "#e9c19c" : "#eec5a2"} />
      <path d="M34 50c-2 2-2 5 0 7M86 50c2 2 2 5 0 7" fill="none" stroke="#c08f6b" strokeWidth="1" strokeLinecap="round" />

      {isDario ? (
        <path d="M60 20c14 0 24 11 24 26 0 8-1 15-4 21-3 8-11 15-20 15s-17-7-20-15c-3-6-4-13-4-21 0-15 10-26 24-26z" fill={`url(#face-${skin})`} />
      ) : (
        <path d="M60 20c14 0 24 11 24 26 0 8-1 15-4 21-3 8-11 14-20 14s-17-6-20-14c-3-6-4-13-4-21 0-15 10-26 24-26z" fill={`url(#face-${skin})`} />
      )}

      {/* Soft form shading: temples, cheeks, under the lower lip. */}
      <path d="M40 44c-2 8-2 16 1 23-4-6-5-16-1-23zM80 44c2 8 2 16-1 23 4-6 5-16 1-23z" fill="#c9976f" opacity="0.35" />
      <ellipse cx="46" cy="60" rx="6" ry="4" fill="#e0a982" opacity="0.3" />
      <ellipse cx="74" cy="60" rx="6" ry="4" fill="#e0a982" opacity="0.3" />
      <ellipse cx="60" cy="75" rx="5" ry="2.5" fill="#c9976f" opacity="0.25" />

      {isDario ? (
        <>
          {/* Signature dark curls: one volume, lumpy silhouette rather than exaggerated. */}
          <g fill={`url(#hair-${skin})`}>
            <circle cx="41" cy="27" r="10" />
            <circle cx="52" cy="20" r="11" />
            <circle cx="65" cy="19" r="11" />
            <circle cx="77" cy="25" r="10" />
            <circle cx="83" cy="36" r="8" />
            <circle cx="36" cy="38" r="8" />
            <path d="M35 45c-2-18 10-30 25-30s27 12 25 30c-2-8-4-13-8-16-5-4-11-6-17-6s-12 2-17 6c-4 3-6 8-8 16z" />
          </g>
          <g fill="none" stroke="#1f1611" strokeWidth="1.6" strokeLinecap="round" opacity="0.55">
            <path d="M45 26c3-3 7-3 9 0" />
            <path d="M60 21c3-2 7-1 9 2" />
            <path d="M74 28c3-2 6-1 7 2" />
            <path d="M39 34c2-3 5-3 7-1" />
          </g>
          <path d="M42 44c4-4 10-5 14-3M64 41c4-2 10-1 14 3" fill="none" stroke="#2b1f19" strokeWidth="3.2" strokeLinecap="round" />

          {/* Eyes */}
          <g>
            <path d="M44 50c1.6-3.6 4.6-5.2 6.2-5.2s4.6 1.6 6.2 5.2c-1.6 3.4-4.6 4.8-6.2 4.8s-4.6-1.4-6.2-4.8z" fill="#ffffff" />
            <circle cx="50.2" cy="50" r="2.9" fill="#6b4a33" />
            <circle cx="50.2" cy="50" r="1.3" fill="#20160f" />
            <circle cx="51.3" cy="48.9" r="0.9" fill="#ffffff" opacity="0.9" />
            <path d="M44 50c1.6-3.6 4.6-5.2 6.2-5.2s4.6 1.6 6.2 5.2" fill="none" stroke="#4a352c" strokeWidth="1.4" strokeLinecap="round" />
            <path d="M63.6 49c1.6-3.6 4.6-5.2 6.2-5.2s4.6 1.6 6.2 5.2c-1.6 3.4-4.6 4.8-6.2 4.8s-4.6-1.4-6.2-4.8z" fill="#ffffff" />
            <circle cx="69.8" cy="49" r="2.9" fill="#6b4a33" />
            <circle cx="69.8" cy="49" r="1.3" fill="#20160f" />
            <circle cx="70.9" cy="47.9" r="0.9" fill="#ffffff" opacity="0.9" />
            <path d="M63.6 49c1.6-3.6 4.6-5.2 6.2-5.2s4.6 1.6 6.2 5.2" fill="none" stroke="#4a352c" strokeWidth="1.4" strokeLinecap="round" />
          </g>

          {/* Rectangular dark frames, straight and symmetric. */}
          <g fill="none" stroke="#2c3138" strokeWidth="2.4">
            <rect x="41" y="43" width="19" height="14" rx="4.5" />
            <rect x="61" y="42" width="19" height="14" rx="4.5" />
            <path d="M60 49h1" />
            <path d="M41 47l-6 2M80 46l5 2" />
          </g>

          <path d="M59 51c-1 6-3 9-4 11 0 2 2 3 5 3s5-1 5-3c-1-2-3-5-4-11" fill="none" stroke="#c08f6b" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
          <path d="M56 66c1.4 0.8 6.6 0.8 8 0" fill="none" stroke="#b9835f" strokeWidth="1" strokeLinecap="round" opacity="0.7" />

          {/* Warm, natural smile. */}
          <path d="M50 72c4 4 16 4 20 0" fill="none" stroke="#a5654f" strokeWidth="2.2" strokeLinecap="round" />
          <path d="M52 73c4 3 12 3 16 0-4 4-12 4-16 0z" fill="#f4ece0" />
          <path d="M49 71c4-1.5 18-1.5 22 0" fill="none" stroke="#c9976f" strokeWidth="0.9" opacity="0.6" />
          <path d="M45 66c1 5 4 9 6 11M75 65c-1 5-4 9-6 11" fill="none" stroke="#c9976f" strokeWidth="1.2" strokeLinecap="round" opacity="0.5" />
          <path d="M46 68c2 8 7 13 14 13s12-5 14-13c2 8-4 17-14 17s-16-9-14-17z" fill="#6b4b3a" opacity="0.16" />
        </>
      ) : (
        <>
          {/* Full short hair, side-parted, sitting naturally on the forehead. */}
          <path d="M36 46c-3-19 9-31 24-31s27 12 24 31c-2-8-4-14-8-17-5 4-11 6-18 6-6 0-11-1-15-4-3 4-6 8-7 15z" fill={`url(#hair-${skin})`} />
          <path d="M37 40c1 4 1 7 1 10-2-3-2-7-1-10zM83 40c1 3 1 7-1 10 0-3 0-6 1-10z" fill="#4c3a2c" />
          <path d="M41 27c6-7 15-10 24-8 5 1 9 4 12 8-5-6-11-9-18-9-7 0-13 3-18 9z" fill="#8b7157" opacity="0.45" />
          <g fill="none" stroke="#3f3025" strokeWidth="1.2" strokeLinecap="round" opacity="0.4">
            <path d="M47 24c5-4 12-5 18-4" />
            <path d="M52 19c5-2 10-2 15 0" />
          </g>
          <path d="M43 43c4-3 10-4 14-2M63 41c4-2 10-1 14 2" fill="none" stroke="#4c3a2c" strokeWidth="2.8" strokeLinecap="round" />

          {/* Eyes */}
          <g>
            <path d="M44 49c1.7-3.7 4.7-5.3 6.3-5.3s4.6 1.6 6.3 5.3c-1.7 3.5-4.7 4.9-6.3 4.9s-4.6-1.4-6.3-4.9z" fill="#ffffff" />
            <circle cx="50.3" cy="49" r="3" fill="#5f8296" />
            <circle cx="50.3" cy="49" r="1.3" fill="#1d262c" />
            <circle cx="51.4" cy="47.9" r="0.9" fill="#ffffff" opacity="0.9" />
            <path d="M44 49c1.7-3.7 4.7-5.3 6.3-5.3s4.6 1.6 6.3 5.3" fill="none" stroke="#4c3a2c" strokeWidth="1.4" strokeLinecap="round" />
            <path d="M63.4 48.4c1.7-3.7 4.7-5.3 6.3-5.3s4.6 1.6 6.3 5.3c-1.7 3.5-4.7 4.9-6.3 4.9s-4.6-1.4-6.3-4.9z" fill="#ffffff" />
            <circle cx="69.7" cy="48.4" r="3" fill="#5f8296" />
            <circle cx="69.7" cy="48.4" r="1.3" fill="#1d262c" />
            <circle cx="70.8" cy="47.3" r="0.9" fill="#ffffff" opacity="0.9" />
            <path d="M63.4 48.4c1.7-3.7 4.7-5.3 6.3-5.3s4.6 1.6 6.3 5.3" fill="none" stroke="#4c3a2c" strokeWidth="1.4" strokeLinecap="round" />
          </g>
          <path d="M45 55c2 1.6 8 1.6 10 0M64 54.4c2 1.6 8 1.6 10 0" fill="none" stroke="#cfa17a" strokeWidth="1" strokeLinecap="round" opacity="0.7" />

          <path d="M59.6 52c-0.8 5-2.4 7.5-3.4 9.5 0 1.8 1.8 2.7 4.4 2.7s4.4-0.9 4.2-2.7c-0.9-2-2.6-4.5-3.4-9.5" fill="none" stroke="#c08f6b" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
          <path d="M56.6 64.4c1.3 0.7 5.9 0.7 7.2 0" fill="none" stroke="#b9835f" strokeWidth="1" strokeLinecap="round" opacity="0.7" />

          {/* Relaxed, closed-mouth smile. */}
          <path d="M51 72c4 3.5 14 3.5 18 0" fill="none" stroke="#a5654f" strokeWidth="2.2" strokeLinecap="round" />
          <path d="M50 71.5c0.6-1 1.6-1.4 2.4-1.2M70 71c-0.6-1-1.6-1.4-2.4-1.2" fill="none" stroke="#c9976f" strokeWidth="1" strokeLinecap="round" />
          <path d="M46 65c1 5 3 9 5 11M74 64c-1 5-3 9-5 11" fill="none" stroke="#c9976f" strokeWidth="1.2" strokeLinecap="round" opacity="0.45" />

          {/* Grey hoodie with drawstrings. */}
          <path d="M30 100c7-12 17-18 30-18s23 6 30 18c-9-8-19-12-30-12s-21 4-30 12z" fill="#6f7780" />
          <path d="M53 96v14M67 96v14" fill="none" stroke="#dfe3e8" strokeWidth="2" strokeLinecap="round" />
          <circle cx="53" cy="112" r="1.6" fill="#dfe3e8" />
          <circle cx="67" cy="112" r="1.6" fill="#dfe3e8" />
        </>
      )}
    </svg>
  );
}

function drawCharacter(
  context: CanvasRenderingContext2D,
  y: number,
  velocity: number,
  skin: CharacterSkin,
) {
  const tilt = Math.max(-0.35, Math.min(0.65, velocity / 700));
  context.save();
  context.translate(BIRD_X, y);
  context.rotate(tilt);

  context.fillStyle = skin === "dario" ? "#c96545" : "#147d84";
  context.beginPath();
  context.ellipse(-4, 3, 22, 16, 0, 0, Math.PI * 2);
  context.fill();

  context.fillStyle = skin === "dario" ? "#e9a163" : "#8fc7bb";
  context.beginPath();
  context.ellipse(-14, 6, 10, 7, -0.4, 0, Math.PI * 2);
  context.fill();

  context.fillStyle = skin === "dario" ? "#efbd99" : "#f1c09d";
  context.beginPath();
  context.ellipse(8, -3, 13, 15, 0, 0, Math.PI * 2);
  context.fill();

  if (skin === "dario") {
    // Dark curls, drawn as one overlapping volume.
    context.fillStyle = "#2f2320";
    for (const [x, yOffset, radius] of [
      [-6, -4, 5],
      [-5, -10, 6],
      [0, -15, 6.5],
      [7, -17, 6.5],
      [14, -15, 6],
      [19, -11, 4.5],
    ] as const) {
      context.beginPath();
      context.arc(x, yOffset, radius, 0, Math.PI * 2);
      context.fill();
    }

    // Side view: only the near lens is visible. The temple arm runs back
    // along the temple into the hair, and the bridge crosses the nose.
    context.strokeStyle = "#2c3138";
    context.lineWidth = 1.3;
    context.strokeRect(12, -9.5, 8, 7);
    context.beginPath();
    context.moveTo(12, -8.6);
    context.lineTo(-1, -7);
    context.moveTo(20, -6.4);
    context.lineTo(21.6, -5.6);
    context.stroke();

    context.fillStyle = "#ffffff";
    context.beginPath();
    context.arc(16, -6, 2.2, 0, Math.PI * 2);
    context.fill();
    context.fillStyle = "#6b4a33";
    context.beginPath();
    context.arc(16.4, -6, 1.3, 0, Math.PI * 2);
    context.fill();
    context.fillStyle = "#17212b";
    context.beginPath();
    context.arc(16.6, -6, 0.6, 0, Math.PI * 2);
    context.fill();

    context.strokeStyle = "#c08f6b";
    context.lineWidth = 1.2;
    context.beginPath();
    context.moveTo(18.5, -1.5);
    context.quadraticCurveTo(20, 1, 18, 1.8);
    context.stroke();

    // Warm smile.
    context.strokeStyle = "#a5654f";
    context.lineWidth = 1.5;
    context.beginPath();
    context.moveTo(9, 5);
    context.quadraticCurveTo(13.5, 8.5, 17.5, 4.5);
    context.stroke();
  } else {
    // Hood, then short side-swept hair.
    context.strokeStyle = "#8d959e";
    context.lineWidth = 4;
    context.beginPath();
    context.arc(6, -1, 15, Math.PI * 0.62, Math.PI * 1.45);
    context.stroke();
    context.strokeStyle = "#dfe3e8";
    context.lineWidth = 1.2;
    context.beginPath();
    context.moveTo(-3, 10);
    context.lineTo(-4, 15);
    context.moveTo(2, 11);
    context.lineTo(2, 16);
    context.stroke();

    context.fillStyle = "#5b4736";
    context.beginPath();
    context.arc(8, -9, 12, Math.PI * 1.03, Math.PI * 1.97);
    context.closePath();
    context.fill();
    context.beginPath();
    context.moveTo(19, -12);
    context.quadraticCurveTo(17, -7, 12, -6.5);
    context.quadraticCurveTo(16, -9, 16.5, -13);
    context.closePath();
    context.fill();

    context.strokeStyle = "#46352b";
    context.lineWidth = 1.6;
    context.beginPath();
    context.moveTo(11, -8.5);
    context.lineTo(17, -8);
    context.stroke();

    context.fillStyle = "#ffffff";
    context.beginPath();
    context.arc(15.5, -4.5, 2.6, 0, Math.PI * 2);
    context.fill();
    context.fillStyle = "#5f8296";
    context.beginPath();
    context.arc(16, -4.5, 1.5, 0, Math.PI * 2);
    context.fill();
    context.fillStyle = "#1d262c";
    context.beginPath();
    context.arc(16.3, -4.5, 0.7, 0, Math.PI * 2);
    context.fill();

    context.strokeStyle = "#c08f6b";
    context.lineWidth = 1.2;
    context.beginPath();
    context.moveTo(18.5, 0);
    context.quadraticCurveTo(20, 2.4, 18, 3.2);
    context.stroke();

    context.strokeStyle = "#a5654f";
    context.lineWidth = 1.5;
    context.beginPath();
    context.moveTo(9.5, 6);
    context.quadraticCurveTo(13.5, 9, 17.5, 5.5);
    context.stroke();
  }

  context.fillStyle = "#f0a32f";
  context.beginPath();
  context.moveTo(19, 0);
  context.lineTo(31, 4);
  context.lineTo(19, 7);
  context.closePath();
  context.fill();

  context.fillStyle = "#ffffff";
  context.font = "bold 10px ui-sans-serif, system-ui, sans-serif";
  context.textAlign = "center";
  context.fillText(skin === "dario" ? "D" : "S", -6, 7);
  context.restore();
}

function drawCloud(
  context: CanvasRenderingContext2D,
  x: number,
  y: number,
  scale: number,
) {
  context.beginPath();
  context.arc(x, y, 18 * scale, 0, Math.PI * 2);
  context.arc(x + 22 * scale, y - 7 * scale, 25 * scale, 0, Math.PI * 2);
  context.arc(x + 49 * scale, y + 1 * scale, 17 * scale, 0, Math.PI * 2);
  context.fill();
}

function quadraticBezierY(start: number, control: number, end: number, t: number) {
  const remaining = 1 - t;
  return remaining * remaining * start + 2 * remaining * t * control + t * t * end;
}

function drawSanFranciscoBackground(
  context: CanvasRenderingContext2D,
  scroll: number,
) {
  context.fillStyle = "#cfe7e8";
  context.fillRect(0, 0, WIDTH, GROUND_Y);

  const cloudOffset = (scroll * 0.035) % 620;
  context.fillStyle = "rgba(255, 255, 255, 0.72)";
  for (let x = -cloudOffset - 100; x < WIDTH + 200; x += 310) {
    drawCloud(context, x, 62, 0.9);
    drawCloud(context, x + 178, 108, 0.55);
  }

  const hillOffset = (scroll * 0.07) % 620;
  context.fillStyle = "#9ebbb3";
  for (let x = -hillOffset - 620; x < WIDTH + 620; x += 620) {
    context.beginPath();
    context.moveTo(x, 252);
    context.lineTo(x + 92, 174);
    context.lineTo(x + 168, 218);
    context.lineTo(x + 266, 151);
    context.lineTo(x + 382, 222);
    context.lineTo(x + 500, 166);
    context.lineTo(x + 620, 240);
    context.lineTo(x + 620, 258);
    context.lineTo(x, 258);
    context.closePath();
    context.fill();
  }

  const waterline = 258;
  context.fillStyle = "#93c6cc";
  context.fillRect(0, waterline, WIDTH, GROUND_Y - waterline);

  const skylineOffset = (scroll * 0.16) % 560;
  const skylineBase = 258;
  context.fillStyle = "#698985";
  for (let x = -skylineOffset - 560; x < WIDTH + 560; x += 560) {
    context.fillRect(x + 18, skylineBase - 66, 38, 66);
    context.fillRect(x + 64, skylineBase - 87, 44, 87);
    context.fillRect(x + 116, skylineBase - 56, 29, 56);
    context.fillRect(x + 228, skylineBase - 77, 54, 77);
    context.fillRect(x + 292, skylineBase - 62, 34, 62);
    context.fillRect(x + 380, skylineBase - 90, 46, 90);
    context.fillRect(x + 437, skylineBase - 68, 33, 68);

    // Transamerica Pyramid.
    context.beginPath();
    context.moveTo(x + 151, skylineBase);
    context.lineTo(x + 183, 126);
    context.lineTo(x + 215, skylineBase);
    context.closePath();
    context.fill();
    context.fillRect(x + 170, 192, 26, 4);
    context.fillRect(x + 164, 218, 38, 4);

    // Salesforce Tower and Coit Tower silhouettes.
    context.beginPath();
    context.arc(x + 350, 173, 17, Math.PI, Math.PI * 2);
    context.fill();
    context.fillRect(x + 333, 173, 34, skylineBase - 173);
    context.fillRect(x + 490, 194, 18, skylineBase - 194);
    context.fillRect(x + 486, 191, 26, 7);
  }

  // A continuous waterfront keeps the skyline visibly above the bay.
  context.fillStyle = "#587773";
  context.fillRect(0, skylineBase, WIDTH, 7);
  context.fillStyle = "#b8b18f";
  context.fillRect(0, skylineBase + 7, WIDTH, 3);

  context.fillStyle = "rgba(255, 255, 255, 0.3)";
  for (let x = -((scroll * 0.2) % 52); x < WIDTH; x += 52) {
    context.fillRect(x, 289 + ((Math.floor(x / 52) % 2 + 2) % 2) * 15, 28, 2);
  }

  const bridgeOffset = (scroll * 0.31) % 760;
  for (let x = -bridgeOffset - 25; x < WIDTH + 760; x += 760) {
    const leftAnchor = x + 25;
    const leftTower = x + 130;
    const rightTower = x + 390;
    const rightAnchor = x + 500;
    const anchorCableY = 198;
    const towerTop = 148;
    const towerBottom = 282;
    const deckY = 236;
    const bridgeColor = "#b85846";
    const cableControlY = 226;
    context.strokeStyle = "#b85846";
    context.fillStyle = bridgeColor;

    // Twin tower legs continue below the deck into the bay.
    for (const tower of [leftTower, rightTower]) {
      context.fillRect(tower - 13, towerTop, 9, towerBottom - towerTop);
      context.fillRect(tower + 4, towerTop, 9, towerBottom - towerTop);
      context.fillRect(tower - 15, towerTop + 9, 30, 6);
      context.fillRect(tower - 15, towerTop + 42, 30, 6);
      context.fillRect(tower - 15, deckY - 9, 30, 6);
      context.fillRect(tower - 17, towerBottom - 4, 34, 7);
    }

    // Concrete shoreline anchorages give both cable ends a visible load path.
    context.fillStyle = "#77736c";
    for (const anchor of [leftAnchor, rightAnchor]) {
      context.fillRect(anchor - 13, anchorCableY, 26, skylineBase + 10 - anchorCableY);
      context.fillRect(anchor - 17, anchorCableY - 5, 34, 8);
      context.fillRect(anchor - 17, skylineBase + 4, 34, 7);
    }
    context.fillStyle = bridgeColor;

    // One continuous main cable terminates at both anchors and both tower tops.
    context.lineWidth = 4;
    context.beginPath();
    context.moveTo(leftAnchor, anchorCableY);
    context.quadraticCurveTo(leftAnchor + 58, 194, leftTower, towerTop);
    context.quadraticCurveTo((leftTower + rightTower) / 2, cableControlY, rightTower, towerTop);
    context.quadraticCurveTo(rightAnchor - 58, 194, rightAnchor, anchorCableY);
    context.stroke();

    // Suspenders begin on the exact central cable curve and end on the deck.
    context.lineWidth = 1;
    for (let cableX = leftTower + 20; cableX < rightTower; cableX += 20) {
      const t = (cableX - leftTower) / (rightTower - leftTower);
      const cableY = quadraticBezierY(towerTop, cableControlY, towerTop, t);
      context.beginPath();
      context.moveTo(cableX, cableY);
      context.lineTo(cableX, deckY);
      context.stroke();
    }

    // A continuous approach deck joins adjacent bridge repeats, so the span
    // always exits the viewport or reaches an anchorage instead of floating.
    context.fillRect(x - 130, deckY, 760, 7);
  }
}

function drawWorld(
  context: CanvasRenderingContext2D,
  world: World,
  skin: CharacterSkin,
  status: GameStatus,
) {
  context.clearRect(0, 0, WIDTH, HEIGHT);
  drawSanFranciscoBackground(context, world.scroll);

  world.pipes.forEach((pipe) => drawPipe(context, pipe));
  drawCharacter(context, world.birdY, world.velocity, skin);

  context.fillStyle = "#d6b96d";
  context.fillRect(0, GROUND_Y, WIDTH, HEIGHT - GROUND_Y);
  context.fillStyle = "#f4d785";
  context.fillRect(0, GROUND_Y, WIDTH, 7);
  context.fillStyle = "rgba(77, 65, 36, 0.25)";
  const groundOffset = (world.scroll * 0.78) % 30;
  for (let x = -groundOffset - 10; x < WIDTH; x += 30) {
    context.fillRect(x, GROUND_Y + 15, 18, 3);
  }

  context.textAlign = "center";
  context.fillStyle = "rgba(20, 37, 43, 0.78)";
  context.font = "800 34px ui-sans-serif, system-ui, sans-serif";
  context.fillText(String(world.score), WIDTH / 2, 49);

  if (status !== "playing") {
    const panelY = status === "over" ? 128 : 82;
    context.fillStyle = "rgba(17, 24, 39, 0.72)";
    context.fillRect(115, panelY, 250, status === "over" ? 91 : 72);
    context.fillStyle = "#ffffff";
    context.font = "700 18px ui-sans-serif, system-ui, sans-serif";
    context.fillText(status === "over" ? "REVISE AND RESUBMIT" : "READY TO REVIEW?", WIDTH / 2, panelY + 30);
    context.font = "500 12px ui-sans-serif, system-ui, sans-serif";
    context.fillText(
      status === "over" ? `Score ${world.score} · Space or click to retry` : "Space, ↑, or click to flap",
      WIDTH / 2,
      panelY + 54,
    );
  }
}

function collides(world: World): boolean {
  if (world.birdY - BIRD_RADIUS <= 0 || world.birdY + BIRD_RADIUS >= GROUND_Y) {
    return true;
  }
  return world.pipes.some((pipe) => {
    const overlapsHorizontally =
      BIRD_X + BIRD_RADIUS > pipe.x && BIRD_X - BIRD_RADIUS < pipe.x + PIPE_WIDTH;
    if (!overlapsHorizontally) return false;
    const gapTop = pipe.gapY - PIPE_GAP / 2;
    const gapBottom = pipe.gapY + PIPE_GAP / 2;
    return world.birdY - BIRD_RADIUS < gapTop || world.birdY + BIRD_RADIUS > gapBottom;
  });
}

export default function FlappyBirdGame({ onClose }: Props) {
  const dialogRef = useModalDialog<HTMLDivElement>(onClose);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const firstPilotRef = useRef<HTMLButtonElement>(null);
  const worldRef = useRef<World>(newWorld());
  const skinRef = useRef<CharacterSkin>("dario");
  const [skin, setSkin] = useState<CharacterSkin | null>(null);
  const [status, setStatus] = useState<GameStatus>("ready");
  const [score, setScore] = useState(0);
  const [highScore, setHighScore] = useState(() => readFlappyHighScore());
  if (skin) skinRef.current = skin;

  useEffect(() => {
    if (skin) return;
    const focusPilot = window.setTimeout(() => firstPilotRef.current?.focus(), 0);
    return () => window.clearTimeout(focusPilot);
  }, [skin]);

  const finishGame = useCallback(() => {
    setStatus("over");
    const finalScore = worldRef.current.score;
    setHighScore((current) => persistFlappyHighScore(finalScore, current));
  }, []);

  const startGame = useCallback(() => {
    if (!skin) return;
    worldRef.current = newWorld();
    setScore(0);
    setStatus("playing");
  }, [skin]);

  const choosePilot = useCallback((choice: CharacterSkin) => {
    worldRef.current = newWorld();
    setScore(0);
    setStatus("ready");
    setSkin(choice);
  }, []);

  const changeCharacter = useCallback(() => {
    worldRef.current = newWorld();
    setScore(0);
    setStatus("ready");
    setSkin(null);
  }, []);

  const flap = useCallback(() => {
    if (!skin) return;
    if (status === "playing") {
      worldRef.current.velocity = FLAP_VELOCITY;
    } else {
      startGame();
    }
  }, [skin, startGame, status]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== " " && event.key !== "ArrowUp") return;
      if (!skin) return;
      event.preventDefault();
      flap();
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [flap, skin, status]);

  useEffect(() => {
    if (!skin || status === "playing") return;
    const context = canvasRef.current?.getContext("2d");
    if (context) drawWorld(context, worldRef.current, skin, status);
  }, [skin, status]);

  useEffect(() => {
    if (!skin || status !== "playing") return;
    const context = canvasRef.current?.getContext("2d");
    if (!context) return;

    let animationFrame = 0;
    worldRef.current.lastTime = performance.now();

    const animate = (time: number) => {
      const world = worldRef.current;
      const elapsed = Math.min((time - world.lastTime) / 1000, 0.034);
      world.lastTime = time;
      world.velocity += GRAVITY * elapsed;
      world.birdY += world.velocity * elapsed;
      world.spawnIn -= elapsed;
      world.scroll += PIPE_SPEED * elapsed;

      if (world.spawnIn <= 0) {
        const minGapCenter = PIPE_GAP / 2 + 35;
        const maxGapCenter = GROUND_Y - PIPE_GAP / 2 - 35;
        world.pipes.push({
          x: WIDTH + 10,
          gapY: minGapCenter + Math.random() * (maxGapCenter - minGapCenter),
          counted: false,
        });
        world.spawnIn += PIPE_INTERVAL;
      }

      for (const pipe of world.pipes) {
        pipe.x -= PIPE_SPEED * elapsed;
        if (!pipe.counted && pipe.x + PIPE_WIDTH < BIRD_X) {
          pipe.counted = true;
          world.score += 1;
          setScore(world.score);
        }
      }
      world.pipes = world.pipes.filter((pipe) => pipe.x + PIPE_WIDTH > -10);
      drawWorld(context, world, skinRef.current, "playing");

      if (collides(world)) {
        finishGame();
        return;
      }
      animationFrame = window.requestAnimationFrame(animate);
    };

    drawWorld(context, worldRef.current, skinRef.current, "playing");
    animationFrame = window.requestAnimationFrame(animate);
    return () => window.cancelAnimationFrame(animationFrame);
  }, [finishGame, skin, status]);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-3 sm:p-6"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="flappy-pipeline-title"
        tabIndex={-1}
        className="w-full max-w-2xl overflow-hidden rounded-2xl border border-emerald-950/20 bg-[#f8fbfa] shadow-2xl dark:border-white/10 dark:bg-gray-950"
      >
        <header className="flex items-start justify-between gap-4 border-b border-emerald-950/10 px-4 py-3 dark:border-white/10 sm:px-5">
          <div>
            <p className="text-[10px] font-bold uppercase tracking-[0.18em] text-emerald-700 dark:text-emerald-400">
              Classified appendix
            </p>
            <h2 id="flappy-pipeline-title" className="mt-0.5 text-xl font-black tracking-tight text-gray-950 dark:text-white">
              Flappy Pipeline
            </h2>
          </div>
          <div className="flex items-center gap-4">
            <div className="text-right text-[11px] font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400">
              <div>Score <span aria-live="polite" className="tabular-nums text-gray-900 dark:text-gray-100">{score}</span></div>
              <div>Best <span className="tabular-nums text-emerald-700 dark:text-emerald-400">{highScore}</span></div>
            </div>
            <button
              type="button"
              onClick={onClose}
              aria-label="Close Flappy Pipeline"
              className="rounded-lg p-1.5 text-gray-400 hover:bg-gray-200 hover:text-gray-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500 dark:hover:bg-gray-800 dark:hover:text-white"
            >
              ✕
            </button>
          </div>
        </header>

        <div className="p-3 sm:p-5">
          {!skin ? (
            <section
              aria-labelledby="choose-pilot-title"
              className="rounded-xl border border-emerald-950/15 bg-[#edf6f3] px-4 py-6 dark:border-white/10 dark:bg-gray-900 sm:px-8 sm:py-8"
            >
              <div className="text-center">
                <p className="text-[10px] font-semibold uppercase tracking-[0.16em] text-emerald-700 dark:text-emerald-400">
                  New round
                </p>
                <h3 id="choose-pilot-title" className="mt-1 text-2xl font-black tracking-tight text-gray-950 dark:text-white">
                  Choose your pilot
                </h3>
                <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
                  Pick a character to begin.
                </p>
              </div>

              <div className="mx-auto mt-6 grid max-w-lg grid-cols-2 gap-3 sm:gap-5" role="group" aria-label="Pilot choice">
                {(["dario", "sam"] as const).map((pilot) => (
                  <button
                    key={pilot}
                    ref={pilot === "dario" ? firstPilotRef : undefined}
                    type="button"
                    data-autofocus={pilot === "dario" ? true : undefined}
                    aria-label={`Choose ${pilot.toUpperCase()}`}
                    onClick={() => choosePilot(pilot)}
                    className="group flex flex-col items-center rounded-2xl border border-gray-200 bg-white px-3 py-4 text-center shadow-sm transition hover:-translate-y-0.5 hover:border-emerald-500 hover:shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500 dark:border-gray-700 dark:bg-gray-950 dark:hover:border-emerald-400"
                  >
                    <PilotPortrait skin={pilot} />
                    <span className="mt-2 text-sm font-black tracking-[0.12em] text-gray-950 group-hover:text-emerald-800 dark:text-white dark:group-hover:text-emerald-300">
                      {pilot.toUpperCase()}
                    </span>
                  </button>
                ))}
              </div>
            </section>
          ) : (
            <>
              <canvas
                ref={canvasRef}
                width={WIDTH}
                height={HEIGHT}
                tabIndex={0}
                aria-label={`Flappy Pipeline game board. ${status === "over" ? "Game over." : status === "playing" ? "Game in progress." : "Ready to play."}`}
                onPointerDown={flap}
                className="block aspect-[4/3] w-full cursor-pointer rounded-xl border border-emerald-950/20 bg-[#dff2ef] shadow-inner outline-none focus-visible:ring-2 focus-visible:ring-emerald-500 dark:border-white/10"
              />

              <div className="mt-4 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <div className="flex items-center gap-2">
                  <span className="text-[10px] font-semibold uppercase tracking-wider text-gray-500 dark:text-gray-400">Pilot</span>
                  <span className="rounded-full bg-emerald-700 px-3 py-1.5 text-xs font-bold tracking-wide text-white dark:bg-emerald-400 dark:text-gray-950">
                    {skin.toUpperCase()}
                  </span>
                  {status !== "playing" && (
                    <button
                      type="button"
                      onClick={changeCharacter}
                      className="rounded px-2 py-1 text-[11px] font-medium text-gray-500 hover:bg-gray-100 hover:text-gray-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500 dark:hover:bg-gray-800 dark:hover:text-white"
                    >
                      Change character
                    </button>
                  )}
                </div>

                <div className="flex items-center justify-between gap-3 sm:justify-end">
                  <p className="text-[11px] text-gray-500 dark:text-gray-400">
                    Space / ↑ / click
                  </p>
                  {status === "ready" && (
                    <button
                      type="button"
                      data-autofocus
                      onClick={startGame}
                      className="rounded-lg bg-gray-950 px-4 py-2 text-xs font-bold text-white shadow-sm hover:bg-emerald-800 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500 dark:bg-gray-100 dark:text-gray-950 dark:hover:bg-emerald-300"
                    >
                      Start flight
                    </button>
                  )}
                  {status === "over" && (
                    <button
                      type="button"
                      onClick={startGame}
                      className="rounded-lg bg-gray-950 px-4 py-2 text-xs font-bold text-white shadow-sm hover:bg-emerald-800 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500 dark:bg-gray-100 dark:text-gray-950 dark:hover:bg-emerald-300"
                    >
                      Try again
                    </button>
                  )}
                </div>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
