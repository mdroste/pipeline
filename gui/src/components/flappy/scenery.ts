import { GROUND_Y, HEIGHT, WIDTH } from "./model";

function drawCloud(
  context: CanvasRenderingContext2D,
  x: number,
  y: number,
  scale: number,
) {
  context.save();
  context.translate(x, y);
  context.scale(scale, scale);
  context.fillStyle = "#f8f8ed";
  context.beginPath();
  context.moveTo(-32, 9);
  context.quadraticCurveTo(-40, -5, -22, -9);
  context.bezierCurveTo(-21, -30, 11, -33, 17, -12);
  context.quadraticCurveTo(39, -17, 43, 2);
  context.quadraticCurveTo(59, 4, 51, 12);
  context.lineTo(-25, 12);
  context.closePath();
  context.fill();
  context.fillStyle = "#e4eee7";
  context.fillRect(-23, 10, 65, 2);
  context.restore();
}

function drawHills(context: CanvasRenderingContext2D, scroll: number) {
  context.fillStyle = "#abc5b8";
  for (let x = -((scroll * 0.055) % 720) - 720; x < WIDTH; x += 720) {
    context.beginPath();
    context.moveTo(x, 260);
    context.bezierCurveTo(x + 60, 258, x + 96, 197, x + 148, 204);
    context.bezierCurveTo(x + 204, 207, x + 238, 255, x + 306, 240);
    context.bezierCurveTo(x + 382, 220, x + 409, 212, x + 455, 236);
    context.bezierCurveTo(x + 524, 262, x + 648, 235, x + 720, 260);
    context.lineTo(x + 720, 275);
    context.lineTo(x, 275);
    context.closePath();
    context.fill();
  }
}

function drawSkyline(context: CanvasRenderingContext2D, scroll: number) {
  const base = 268;
  for (let x = -((scroll * 0.1) % 840) - 840; x < WIDTH; x += 840) {
    context.fillStyle = "#91b3ae";
    for (const [offset, width, height] of [
      [270, 19, 26],
      [295, 24, 38],
      [325, 17, 28],
      [382, 25, 35],
      [413, 16, 24],
      [462, 27, 39],
      [494, 18, 30],
      [520, 23, 23],
    ]) {
      context.fillRect(x + offset, base - height, width, height);
      context.fillStyle = "#b0c9bf";
      context.fillRect(x + offset + 3, base - height + 4, 2, height - 4);
      context.fillStyle = "#91b3ae";
    }
    // Small, distinct landmarks sit on the far shore, below the bridge towers.
    context.beginPath();
    context.moveTo(x + 345, base);
    context.lineTo(x + 359, 204);
    context.lineTo(x + 374, base);
    context.closePath();
    context.fill();
    context.fillStyle = "#b7cec3";
    context.beginPath();
    context.moveTo(x + 359, 204);
    context.lineTo(x + 361, base);
    context.lineTo(x + 374, base);
    context.closePath();
    context.fill();
    context.fillStyle = "#91b3ae";
    context.beginPath();
    context.moveTo(x + 435, base);
    context.lineTo(x + 435, 220);
    context.quadraticCurveTo(x + 447, 195, x + 459, 220);
    context.lineTo(x + 459, base);
    context.closePath();
    context.fill();
    context.fillRect(x + 223, 240, 9, 26);
    context.fillRect(x + 221, 238, 13, 4);
    context.fillStyle = "#83aaa2";
    context.beginPath();
    context.moveTo(x + 190, base);
    context.quadraticCurveTo(x + 225, 249, x + 260, base);
    context.lineTo(x + 555, base);
    context.lineTo(x + 555, base + 4);
    context.lineTo(x + 190, base + 4);
    context.closePath();
    context.fill();
  }
}

function cableY(start: number, control: number, end: number, t: number) {
  return (1 - t) ** 2 * start + 2 * (1 - t) * t * control + t ** 2 * end;
}

function drawBridge(context: CanvasRenderingContext2D, x: number) {
  const deck = 249;
  const top = 150;
  const left = x + 120;
  const right = x + 380;
  const start = x - 85;
  const end = x + 585;

  // The center control point is below the deck: the visible cable's low
  // point is y=228, leaving short suspenders at midspan, like a real bridge.
  const spans = [
    [start, left, deck, 240, top],
    [left, right, top, 306, top],
    [right, end, top, 240, deck],
  ];
  context.save();
  context.lineCap = "round";
  context.strokeStyle = "#b66c55";
  context.lineWidth = 1;
  for (const [x1, x2, y1, control, y2] of spans) {
    for (let hanger = x1 + 13; hanger < x2 - 6; hanger += 13) {
      const t = (hanger - x1) / (x2 - x1);
      context.beginPath();
      context.moveTo(hanger, cableY(y1, control, y2, t));
      context.lineTo(hanger, deck);
      context.stroke();
    }
  }

  // The approaches meet low headlands; there are no freestanding anchor towers.
  context.fillStyle = "#799f91";
  for (const anchor of [start, end]) {
    context.beginPath();
    context.moveTo(anchor - 52, 283);
    context.quadraticCurveTo(anchor - 28, 248, anchor, 254);
    context.quadraticCurveTo(anchor + 32, 255, anchor + 54, 283);
    context.closePath();
    context.fill();
  }
  context.fillStyle = "#bbad91";
  context.fillRect(start - 10, deck - 2, 18, 18);
  context.fillRect(end - 8, deck - 2, 18, 18);

  // A shaded deck and a thin railing keep the roadway legible at game scale.
  context.fillStyle = "#934d41";
  context.fillRect(start, deck + 2, end - start, 5);
  context.fillStyle = "#d48a67";
  context.fillRect(start, deck - 1, end - start, 3);
  context.fillStyle = "#af6150";
  for (let post = start; post < end; post += 9) {
    context.fillRect(post, deck - 4, 1, 3);
  }

  // Tapered portal towers, with three open bays and masonry feet in the water.
  for (const tower of [left, right]) {
    context.fillStyle = "#b9ae96";
    context.fillRect(tower - 16, 281, 32, 7);
    context.fillStyle = "#c76a50";
    for (const side of [-1, 1]) {
      const leg = tower + side * 9;
      context.beginPath();
      context.moveTo(leg - 3, top - 4);
      context.lineTo(leg + 3, top - 4);
      context.lineTo(leg + 5, 282);
      context.lineTo(leg - 5, 282);
      context.closePath();
      context.fill();
      context.fillStyle = "#e79b73";
      context.fillRect(leg - 2, top - 2, 1.5, 132);
      context.fillStyle = "#c76a50";
    }
    for (const y of [top + 8, top + 34, top + 65, deck + 5]) {
      context.fillStyle = "#a85544";
      context.fillRect(tower - 10, y, 20, 5);
      context.fillStyle = "#df8863";
      context.fillRect(tower - 10, y, 20, 1.5);
    }
    context.fillStyle = "#efa780";
    context.fillRect(tower - 13, top - 5, 8, 2);
    context.fillRect(tower + 5, top - 5, 8, 2);
    context.fillStyle = "rgba(61, 111, 113, 0.16)";
    context.fillRect(tower - 13, 292, 26, 2);
    context.fillRect(tower - 9, 298, 18, 2);
  }

  context.lineWidth = 2.5;
  context.strokeStyle = "#b65d49";
  for (const [x1, x2, y1, control, y2] of spans) {
    context.beginPath();
    context.moveTo(x1, y1);
    context.quadraticCurveTo((x1 + x2) / 2, control, x2, y2);
    context.stroke();
  }
  context.restore();
}

export function drawSanFranciscoBackground(
  context: CanvasRenderingContext2D,
  scroll: number,
) {
  const sky = context.createLinearGradient(0, 0, 0, 280);
  sky.addColorStop(0, "#b8deda");
  sky.addColorStop(0.7, "#e8edda");
  sky.addColorStop(1, "#f5e6c9");
  context.fillStyle = sky;
  context.fillRect(0, 0, WIDTH, GROUND_Y);
  context.fillStyle = "#faf1cb";
  context.beginPath();
  context.arc(390, 68, 26, 0, Math.PI * 2);
  context.fill();

  for (let x = -((scroll * 0.025) % 600) - 100; x < WIDTH + 100; x += 300) {
    drawCloud(context, x, 65, 0.8);
    drawCloud(context, x + 140, 108, 0.48);
  }
  drawHills(context, scroll);
  const water = context.createLinearGradient(0, 266, 0, GROUND_Y);
  water.addColorStop(0, "#afcfbf");
  water.addColorStop(1, "#78b4b2");
  context.fillStyle = water;
  context.fillRect(0, 266, WIDTH, GROUND_Y - 266);
  drawSkyline(context, scroll);

  // Each ripple stays on its own row as it scrolls, including at tile seams.
  context.fillStyle = "rgba(239, 245, 217, 0.48)";
  for (let row = 0; row < 4; row++) {
    const offset = (scroll * 0.18 + row * 31) % 94;
    for (let x = -offset; x < WIDTH; x += 94) {
      context.fillRect(x, 278 + row * 13, 18 + row * 5, 1.5);
    }
  }
  for (let x = -((scroll * 0.24) % 900) - 915; x < WIDTH + 85; x += 900) {
    drawBridge(context, x);
  }
}

export function drawGround(context: CanvasRenderingContext2D, scroll: number) {
  context.fillStyle = "#ceb98f";
  context.fillRect(0, GROUND_Y, WIDTH, HEIGHT - GROUND_Y);
  context.fillStyle = "#506f60";
  context.fillRect(0, GROUND_Y, WIDTH, 3);
  context.fillStyle = "#a5b77b";
  context.fillRect(0, GROUND_Y + 3, WIDTH, 4);
  context.fillStyle = "#f0dcac";
  context.fillRect(0, GROUND_Y + 7, WIDTH, 3);
  context.fillStyle = "#b39c78";
  context.fillRect(0, GROUND_Y + 10, WIDTH, 1);
  for (let x = -((scroll * 0.78) % 32); x < WIDTH; x += 32) {
    context.fillRect(x, GROUND_Y + 17, 12, 2);
    context.fillRect(x + 18, GROUND_Y + 25, 5, 2);
  }
}
