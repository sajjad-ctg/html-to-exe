// Neon Breakout — a small, complete HTML5 canvas game used to demo
// html-to-exe exports. Everything runs offline: canvas 2D rendering,
// WebAudio for sound, localStorage for the high score, and levels are
// fetched from assets/levels.json through the game:// protocol the
// shell serves at runtime.

const canvas = document.getElementById("game");
const ctx = canvas.getContext("2d");
const W = canvas.width;
const H = canvas.height;

const overlay = document.getElementById("overlay");
const ovTitle = document.getElementById("ov-title");
const ovText = document.getElementById("ov-text");
const ovBtn = document.getElementById("ov-btn");
const hud = {
  score: document.getElementById("score"),
  level: document.getElementById("level"),
  lives: document.getElementById("lives"),
  best: document.getElementById("best"),
};

const BEST_KEY = "neon-breakout-best";
hud.best.textContent = "BEST " + (localStorage.getItem(BEST_KEY) || 0);

// ------------------------------------------------------------- audio

const audioCtx = new (window.AudioContext || window.webkitAudioContext)();
function blip(freq, duration, type) {
  const osc = audioCtx.createOscillator();
  const gain = audioCtx.createGain();
  osc.type = type || "square";
  osc.frequency.value = freq;
  gain.gain.setValueAtTime(0.08, audioCtx.currentTime);
  gain.gain.exponentialRampToValueAtTime(0.001, audioCtx.currentTime + duration);
  osc.connect(gain).connect(audioCtx.destination);
  osc.start();
  osc.stop(audioCtx.currentTime + duration);
}

// -------------------------------------------------------------- state

const state = {
  scene: "menu", // menu | play | paused
  score: 0,
  lives: 3,
  levelIndex: 0,
  levels: [],
  bricks: [],
  particles: [],
  paddle: { x: W / 2 - 55, w: 110, h: 14, y: H - 40 },
  ball: { x: W / 2, y: H - 60, r: 7, vx: 0, vy: 0, speed: 6, stuck: true },
  keys: { left: false, right: false },
};

fetch("assets/levels.json")
  .then((r) => r.json())
  .then((levels) => {
    state.levels = levels;
  })
  .catch((e) => {
    ovText.textContent = "level data failed to load: " + e;
  });

function startLevel(index) {
  const def = state.levels[index % state.levels.length];
  state.bricks = [];
  for (let row = 0; row < def.rows; row++) {
    for (let col = 0; col < def.cols; col++) {
      const solid = def.across === "solid";
      const checker = def.across === "checker" && (row + col) % 2 === 0;
      if (def.across !== "normal" && !solid && !checker) continue;
      state.bricks.push({
        x: 40 + col * ((W - 80) / def.cols),
        y: 70 + row * 26,
        w: (W - 80) / def.cols - 4,
        h: 20,
        hp: def.hp,
        hue: 210 + row * 22,
      });
    }
  }
  hud.level.textContent = "LEVEL " + def.name.toUpperCase();
  resetBall();
}

function resetBall() {
  state.ball.stuck = true;
  state.ball.x = state.paddle.x + state.paddle.w / 2;
  state.ball.y = state.paddle.y - state.ball.r - 2;
  state.ball.vx = 0;
  state.ball.vy = 0;
}

function launch() {
  if (!state.ball.stuck) return;
  state.ball.stuck = false;
  state.ball.vx = state.ball.speed * (Math.random() > 0.5 ? 1 : -1);
  state.ball.vy = -state.ball.speed;
  blip(440, 0.08, "triangle");
}

// --------------------------------------------------------- particles

function burst(x, y, hue) {
  for (let i = 0; i < 12; i++) {
    state.particles.push({
      x, y,
      vx: (Math.random() - 0.5) * 6,
      vy: (Math.random() - 0.5) * 6 - 2,
      life: 1,
      hue,
    });
  }
}

// ------------------------------------------------------------ render

function draw() {
  ctx.clearRect(0, 0, W, H);

  for (const b of state.bricks) {
    ctx.fillStyle = `hsl(${b.hue} 70% 55% / ${0.25 + 0.25 * b.hp})`;
    ctx.fillRect(b.x, b.y, b.w, b.h);
    ctx.strokeStyle = `hsl(${b.hue} 80% 65%)`;
    ctx.strokeRect(b.x + 0.5, b.y + 0.5, b.w - 1, b.h - 1);
  }

  // paddle
  const p = state.paddle;
  ctx.fillStyle = "#6f97ff";
  shadowCtx(() => ctx.fillRect(p.x, p.y, p.w, p.h));

  // ball
  const ball = state.ball;
  ctx.beginPath();
  ctx.arc(ball.x, ball.y, ball.r, 0, Math.PI * 2);
  ctx.fillStyle = "#fff";
  shadowCtx(() => ctx.fill());

  for (const part of state.particles) {
    ctx.globalAlpha = part.life;
    ctx.fillStyle = `hsl(${part.hue} 80% 60%)`;
    ctx.fillRect(part.x - 2, part.y - 2, 4, 4);
  }
  ctx.globalAlpha = 1;
}

function shadowCtx(render) {
  ctx.save();
  ctx.shadowColor = "rgba(111, 151, 255, 0.8)";
  ctx.shadowBlur = 14;
  render();
  ctx.restore();
}

// ------------------------------------------------------------- update

function update() {
  const p = state.paddle;
  const ball = state.ball;

  if (state.keys.left) p.x -= 7;
  if (state.keys.right) p.x += 7;
  p.x = Math.max(8, Math.min(W - p.w - 8, p.x));

  if (ball.stuck) {
    ball.x = p.x + p.w / 2;
    ball.y = p.y - ball.r - 2;
    return;
  }

  ball.x += ball.vx;
  ball.y += ball.vy;

  // walls
  if (ball.x < ball.r + 6 || ball.x > W - ball.r - 6) {
    ball.vx *= -1;
    ball.x = Math.max(ball.r + 6, Math.min(W - ball.r - 6, ball.x));
    blip(220, 0.05);
  }
  if (ball.y < ball.r + 6) {
    ball.vy *= -1;
    blip(220, 0.05);
  }

  // paddle
  if (ball.vy > 0 && ball.y > p.y - ball.r && ball.y < p.y + p.h) {
    if (ball.x > p.x && ball.x < p.x + p.w) {
      // aim buzz: offset from paddle center steers the ball
      const hit = (ball.x - (p.x + p.w / 2)) / (p.w / 2);
      const v = Math.hypot(ball.vx, ball.vy);
      const angle = hit * 1.1 - Math.PI / 2;
      ball.vx = Math.cos(angle) * v;
      ball.vy = Math.sin(angle) * v;
      if (ball.vy > 0) ball.vy *= -1;
      blip(600 + Math.abs(hit) * 200, 0.06, "triangle");
    }
  }

  // bricks
  for (let i = 0; i < state.bricks.length; i++) {
    const b = state.bricks[i];
    if (ball.x > b.x - ball.r && ball.x < b.x + b.w + ball.r &&
        ball.y > b.y - ball.r && ball.y < b.y + b.h + ball.r) {
      state.bricks.splice(i, 1);
      burst(ball.x, ball.y, b.hue);
      blip(300 + b.hp * 200, 0.07);
      state.score += 10 * b.hp;
      hud.score.textContent = "SCORE " + state.score;
      // shallow approach: bounce by overlap axis
      const overlapX = Math.min(ball.x - b.x, b.x + b.w - ball.x);
      const overlapY = Math.min(ball.y - b.y, b.y + b.h - ball.y);
      if (overlapX < overlapY) ball.vx *= -1; else ball.vy *= -1;
      break;
    }
  }

  // fell out
  if (ball.y > H + 20) {
    state.lives--;
    hud.lives.textContent = "LIVES " + state.lives;
    blip(110, 0.4, "sawtooth");
    if (state.lives <= 0) {
      saveBest();
      return showOverlay("GAME OVER", "final score: " + state.score, "RETRY", restart);
    }
    resetBall();
  }

  // level clear
  if (state.bricks.length === 0) {
    blip(880, 0.2, "sine");
    if (state.levelIndex + 1 >= state.levels.length) {
      saveBest();
      return showOverlay("YOU WIN!", "score: " + state.score, "AGAIN", restart);
    }
    state.levelIndex++;
    state.score += 100;
    hud.score.textContent = "SCORE " + state.score;
    startLevel(state.levelIndex);
  }
}

function updateParticles() {
  for (let i = state.particles.length - 1; i >= 0; i--) {
    const part = state.particles[i];
    part.x += part.vx;
    part.y += part.vy;
    part.vy += 0.2;
    part.life -= 0.03;
    if (part.life <= 0) state.particles.splice(i, 1);
  }
}

// -------------------------------------------------------------- flow

// remember the final score as the best across runs
function saveBest() {
  const best = parseInt(localStorage.getItem(BEST_KEY) || "0", 10);
  if (state.score > best) {
    localStorage.setItem(BEST_KEY, String(state.score));
    hud.best.textContent = "BEST " + state.score;
  }
}

function showOverlay(title, text, button, onClick) {
  state.scene = "menu";
  ovTitle.textContent = title;
  ovText.textContent = text;
  ovBtn.textContent = button;
  overlay.classList.remove("hidden");
  ovBtn.onclick = onClick;
}

function restart() {
  state.score = 0;
  state.lives = 3;
  state.levelIndex = 0;
  hud.score.textContent = "SCORE 0";
  hud.lives.textContent = "LIVES 3";
  startLevel(0);
  overlay.classList.add("hidden");
  state.scene = "play";
}

ovBtn.onclick = () => {
  audioCtx.resume();
  restart();
};

// ------------------------------------------------------------- input

window.addEventListener("keydown", (e) => {
  if (e.code === "ArrowLeft") state.keys.left = true;
  if (e.code === "ArrowRight") state.keys.right = true;
  if (e.code === "Space") { e.preventDefault(); launch(); }
  if (e.code === "Escape" && state.scene === "play") {
    showOverlay("PAUSED", "score: " + state.score, "RESUME", () => {
      overlay.classList.add("hidden");
      state.scene = "play";
    });
    // showOverlay sets "menu"; paused keeps the same frozen-look but
    // is distinct so Esc only pauses, never unpauses a paused game
    state.scene = "paused";
  }
});
window.addEventListener("keyup", (e) => {
  if (e.code === "ArrowLeft") state.keys.left = false;
  if (e.code === "ArrowRight") state.keys.right = false;
});
canvas.addEventListener("mousemove", (e) => {
  const rect = canvas.getBoundingClientRect();
  const scale = W / rect.width;
  const x = (e.clientX - rect.left) * scale;
  state.paddle.x = Math.max(8, Math.min(W - state.paddle.w - 8, x - state.paddle.w / 2));
});
canvas.addEventListener("click", launch);

// -------------------------------------------------------------- loop

function frame() {
  if (state.scene === "play") {
    update();
    updateParticles();
  }
  draw();
  requestAnimationFrame(frame);
}
state.scene = "menu";
startLevel(0); // level visible behind the menu overlay
frame();
