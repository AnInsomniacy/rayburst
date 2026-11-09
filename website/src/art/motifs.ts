/**
 * @fileoverview The promo artwork's shared motifs, drawn on a canvas so they
 * can move: the aria (the fine strands of one twisting ribbon that runs flat as
 * a rule, then opens wide while it rises) and a halo, a pool of light that
 * breathes around a mark or a heading. The site draws them from the artwork's
 * numbers, in the dark stage's colours, the light field's or Aria2 Next's gold.
 */

/** A gradient stop along the ribbon: offset 0..1, colour, opacity. */
export type Stop = readonly [offset: number, color: string, alpha: number]

export interface Palette {
  /** The halo's two lights and its strength. */
  halo: readonly [string, string]
  haloAlpha: number
  aria: readonly Stop[]
}

/** The dark stage behind the Rayburst and Connect heroes. */
export const STAGE: Palette = {
  halo: ['#7c3aed', '#c026d3'],
  haloAlpha: 0.32,
  aria: [
    [0, '#c4b5fd', 0],
    [0.25, '#a78bfa', 0.7],
    [0.5, '#e879f9', 1],
    [0.7, '#fdf4ff', 1],
    [0.86, '#a78bfa', 1],
    [1, '#818cf8', 0.9],
  ],
}

/** The light field behind the feature posters and store shots. */
export const FIELD: Palette = {
  halo: ['#b17cec', '#8aa2ff'],
  haloAlpha: 0.3,
  aria: [
    [0, '#c4b5fd', 0],
    [0.25, '#a78bfa', 0.75],
    [0.5, '#9333ea', 1],
    [0.7, '#d946ef', 0.95],
    [0.86, '#7c3aed', 0.9],
    [1, '#6366f1', 0.8],
  ],
}

/** Aria2 Next's engraved paper. */
export const GOLD: Palette = {
  halo: ['#f3cf7a', '#d6a24a'],
  haloAlpha: 0.32,
  aria: [
    [0, '#c8922a', 0],
    [0.12, '#c8922a', 0.7],
    [0.42, '#b47d1c', 1],
    [0.6, '#e3b65a', 1],
    [0.74, '#8e5c10', 1],
    [0.88, '#d5a24a', 1],
    [1, '#6b4206', 1],
  ],
}

export const paletteFor = (theme: 'light' | 'dark') => (theme === 'light' ? FIELD : STAGE)

function rgba(hex: string, alpha: number) {
  const n = parseInt(hex.slice(1), 16)
  return `rgba(${n >> 16},${(n >> 8) & 255},${n & 255},${alpha})`
}

export interface AriaOptions {
  /** Where the ribbon starts and ends along x (it may run past the canvas). */
  x0: number
  end: number
  /** Its line while flat. */
  y: number
  /** Where it starts to open, how wide it opens and how far it rises by `end`. */
  swell: number
  amp: number
  drift?: number
  /** Radians of twist per pixel. */
  twist: number
  /** Rolls the twist along the ribbon; advance it with time to make it flow. */
  phase?: number
  strands?: number
  stroke?: number
  alpha?: number
  /** Open towards the left instead, mirrored about the canvas width `w`. */
  flip?: boolean
  w: number
  paint: readonly Stop[]
}

/** An aria (see the artwork's kit/hero.js). */
export function drawAria(ctx: CanvasRenderingContext2D, o: AriaOptions) {
  const strands = o.strands ?? 64
  const drift = o.drift ?? 0
  const phase = o.phase ?? 0
  const span = Math.max(1, o.end - o.swell)
  const step = 6
  const grad = ctx.createLinearGradient(o.x0, 0, o.end, 0)
  for (const [offset, color, alpha] of o.paint) grad.addColorStop(offset, rgba(color, alpha))
  ctx.save()
  if (o.flip) {
    ctx.translate(o.w, 0)
    ctx.scale(-1, 1)
  }
  ctx.strokeStyle = grad
  ctx.lineWidth = o.stroke ?? 1
  for (let i = 0; i < strands; i++) {
    const phi = (Math.PI * i) / (strands - 1)
    ctx.globalAlpha = (o.alpha ?? 1) * (0.4 + 0.6 * Math.sin(phi))
    ctx.beginPath()
    for (let x = o.x0; x <= o.end + step; x += step) {
      const u = Math.max(0, (x - o.swell) / span)
      const a = 2.5 + o.amp * u ** 1.5
      const y = o.y + a * Math.cos((x - o.x0) * o.twist + phi + phase) - drift * u * u
      if (x === o.x0) ctx.moveTo(x, y)
      else ctx.lineTo(x, y)
    }
    ctx.stroke()
  }
  ctx.restore()
}

export interface HaloOptions {
  cx: number
  cy: number
  /** Radius of the main light. */
  r: number
  /** Seconds on the scene clock. */
  time: number
  palette: Palette
  alpha?: number
}

/**
 * A halo: one light centred on the mark that breathes slowly, and two smaller
 * ones drifting around it at different paces, so the glow shifts without ever
 * drawing a line.
 */
export function drawHalo(ctx: CanvasRenderingContext2D, o: HaloOptions) {
  const a = (o.alpha ?? 1) * o.palette.haloAlpha
  const [main, side] = o.palette.halo
  const t = o.time
  drawBloom(ctx, o.cx, o.cy, o.r * (1 + 0.06 * Math.sin(t * 0.5)), main, a)
  const a1 = t * 0.11
  drawBloom(ctx, o.cx + Math.cos(a1) * o.r * 0.38, o.cy + Math.sin(a1) * o.r * 0.24, o.r * 0.72, side, a * 0.75)
  const a2 = -t * 0.08 + 2.4
  drawBloom(ctx, o.cx + Math.cos(a2) * o.r * 0.42, o.cy + Math.sin(a2) * o.r * 0.28, o.r * 0.58, main, a * 0.6)
}

/** A soft radial glow. */
export function drawBloom(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  r: number,
  color: string,
  alpha: number,
) {
  if (r <= 0 || alpha <= 0) return
  const g = ctx.createRadialGradient(x, y, 0, x, y, r)
  g.addColorStop(0, rgba(color, alpha))
  g.addColorStop(0.4, rgba(color, alpha * 0.45))
  g.addColorStop(1, rgba(color, 0))
  ctx.fillStyle = g
  ctx.fillRect(x - r, y - r, r * 2, r * 2)
}
