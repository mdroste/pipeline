export type CharacterSkin = "dario" | "sam";
export type GameStatus = "ready" | "playing" | "over";

export interface Pipe {
  x: number;
  gapY: number;
  counted: boolean;
}

export interface World {
  birdY: number;
  velocity: number;
  pipes: Pipe[];
  spawnIn: number;
  lastTime: number;
  score: number;
  scroll: number;
}

export const WIDTH = 480;
export const HEIGHT = 360;
export const GROUND_Y = 330;
export const BIRD_X = 108;
export const BIRD_RADIUS = 15;
export const PIPE_WIDTH = 58;
export const PIPE_GAP = 116;
export const PIPE_SPEED = 145;
export const PIPE_INTERVAL = 1.55;
export const GRAVITY = 900;
export const FLAP_VELOCITY = -330;
