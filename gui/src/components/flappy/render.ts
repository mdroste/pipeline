import {
  WIDTH,
  HEIGHT,
  GROUND_Y,
  BIRD_X,
  PIPE_WIDTH,
  PIPE_GAP,
  type CharacterSkin,
  type GameStatus,
  type Pipe,
  type World,
} from "./model";
import { drawSanFranciscoBackground, drawGround } from "./scenery";

function drawPipe(context: CanvasRenderingContext2D, pipe: Pipe) {
  const gapTop = pipe.gapY - PIPE_GAP / 2;
  const gapBottom = pipe.gapY + PIPE_GAP / 2;

  const enamel = context.createLinearGradient(
    pipe.x,
    0,
    pipe.x + PIPE_WIDTH,
    0,
  );
  enamel.addColorStop(0, "#347763");
  enamel.addColorStop(0.23, "#72b894");
  enamel.addColorStop(0.45, "#4a9879");
  enamel.addColorStop(1, "#25624f");
  context.save();
  for (const [y, height, lipY] of [
    [0, gapTop, gapTop - 18],
    [gapBottom, GROUND_Y - gapBottom, gapBottom],
  ]) {
    context.fillStyle = "#224e40";
    context.fillRect(pipe.x, y, PIPE_WIDTH, height);
    context.fillStyle = enamel;
    context.fillRect(pipe.x + 2, y, PIPE_WIDTH - 4, height);
    context.fillStyle = "rgba(213, 242, 192, 0.55)";
    context.fillRect(pipe.x + 9, y, 3, height);
    context.fillStyle = "#224e40";
    context.fillRect(pipe.x - 5, lipY, PIPE_WIDTH + 10, 18);
    context.fillStyle = enamel;
    context.fillRect(pipe.x - 3, lipY + 2, PIPE_WIDTH + 6, 13);
    context.fillStyle = "#a3d0a2";
    context.fillRect(pipe.x - 2, lipY + 2, PIPE_WIDTH + 4, 2);
    context.fillStyle = "#306550";
    context.fillRect(pipe.x - 2, lipY + 13, PIPE_WIDTH + 4, 2);
  }
  context.restore();
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
  context.lineJoin = "round";
  context.lineCap = "round";
  context.lineWidth = 1.6;
  context.strokeStyle = skin === "dario" ? "#753d33" : "#225753";

  // Two tail feathers make the silhouette read as a bird at a glance.
  context.fillStyle = skin === "dario" ? "#dd8856" : "#6aada0";
  context.beginPath();
  context.moveTo(-21, 0);
  context.lineTo(-32, -5);
  context.lineTo(-29, 4);
  context.lineTo(-34, 8);
  context.lineTo(-20, 12);
  context.closePath();
  context.fill();
  context.stroke();

  context.fillStyle = skin === "dario" ? "#c96545" : "#147d84";
  context.beginPath();
  context.ellipse(-4, 3, 22, 16, 0, 0, Math.PI * 2);
  context.fill();
  context.stroke();

  context.fillStyle = skin === "dario" ? "#eeac70" : "#9dcec0";
  context.beginPath();
  context.ellipse(0, 9, 14, 8, -0.2, 0, Math.PI * 2);
  context.fill();

  context.fillStyle = skin === "dario" ? "#e9a163" : "#8fc7bb";
  context.beginPath();
  context.ellipse(-14, 6, 10, 7, -0.4, 0, Math.PI * 2);
  context.fill();
  context.stroke();
  context.beginPath();
  context.moveTo(-19, 7);
  context.quadraticCurveTo(-13, 9, -8, 4);
  context.stroke();

  context.fillStyle = skin === "dario" ? "#efbd99" : "#f1c09d";
  context.beginPath();
  context.ellipse(8, -3, 13, 15, 0, 0, Math.PI * 2);
  context.fill();
  context.strokeStyle = "#b27c60";
  context.lineWidth = 1;
  context.stroke();

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
  context.strokeStyle = "#b97628";
  context.lineWidth = 1.2;
  context.stroke();
  context.beginPath();
  context.moveTo(20, 4);
  context.lineTo(28, 4);
  context.stroke();

  context.restore();
}

export function drawWorld(
  context: CanvasRenderingContext2D,
  world: World,
  skin: CharacterSkin,
  status: GameStatus,
) {
  context.clearRect(0, 0, WIDTH, HEIGHT);
  drawSanFranciscoBackground(context, world.scroll);

  world.pipes.forEach((pipe) => drawPipe(context, pipe));
  drawCharacter(context, world.birdY, world.velocity, skin);

  drawGround(context, world.scroll);

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
    context.fillText(
      status === "over" ? "REVISE AND RESUBMIT" : "READY TO REVIEW?",
      WIDTH / 2,
      panelY + 30,
    );
    context.font = "500 12px ui-sans-serif, system-ui, sans-serif";
    context.fillText(
      status === "over"
        ? `Score ${world.score} · Space or click to retry`
        : "Space, ↑, or click to flap",
      WIDTH / 2,
      panelY + 54,
    );
  }
}
