use ordex::prelude::*;
use rand::Rng;
use wasm_bindgen::prelude::*;
use web_sys::CanvasRenderingContext2d;

// =====================================================================
// 1. Parameters
// =====================================================================
#[derive(Clone, Copy)]
pub struct SimConfig {
    pub pred_init_energy: f32,
    pub pred_energy_drain: f32,
    pub pred_heal_amount: f32,
    pub pred_reproduce_energy: f32,
    pub max_predators: usize,

    pub prey_reproduce_rate: f64,
    pub max_preys: usize,

    pub pred_max_speed: f32,
    pub prey_max_speed: f32,
    pub prey_min_speed: f32,
    pub grid_cell_size: f32,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            pred_init_energy: 1000.0,
            pred_energy_drain: 3.5,
            pred_heal_amount: 30.0,
            pred_reproduce_energy: 1200.0,
            max_predators: 30,

            prey_reproduce_rate: 0.0015,
            max_preys: 4000,

            pred_max_speed: 4.5,
            prey_max_speed: 3.0,
            prey_min_speed: 1.0,
            grid_cell_size: 50.0,
        }
    }
}

// =====================================================================
// 2. Data Structure
// =====================================================================
#[derive(Clone, Copy)]
pub struct Vector2D {
    pub x: f32,
    pub y: f32,
}

impl Vector2D {
    fn distance_sq(&self, other: &Vector2D) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx * dx + dy * dy
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EntityType {
    Prey,
    Predator,
}

pub struct Boid {
    pub position: Vector2D,
    pub velocity: Vector2D,
    pub e_type: EntityType,
    pub is_alive: bool,
    pub energy: f32,
}

#[wasm_bindgen]
pub struct Simulation {
    arena: OrdexArena<Boid>,
    active_indices: Vec<Index>,
    batch_buffer: VerifiedIndices,
    grid_cells: Vec<Vec<Index>>,
    predators_buf: Vec<Index>,
    prey_neighbors_buf: Vec<Index>,
    cell_preds_pos_buf: Vec<Vector2D>,
    width: f32,
    height: f32,
    config: SimConfig,
}

// =====================================================================
// 3. Simulation
// =====================================================================
#[wasm_bindgen]
impl Simulation {
    #[wasm_bindgen(constructor)]
    pub fn new(width: f32, height: f32, initial_count: usize) -> Self {
        let cfg = SimConfig::default();
        let cols = (width / cfg.grid_cell_size).ceil() as usize + 1;
        let rows = (height / cfg.grid_cell_size).ceil() as usize + 1;

        let mut sim = Self {
            arena: OrdexArena::new(),
            active_indices: Vec::new(),
            batch_buffer: VerifiedIndices::new(Vec::new()),
            grid_cells: vec![Vec::with_capacity(32); cols * rows],
            predators_buf: Vec::with_capacity(32),
            prey_neighbors_buf: Vec::with_capacity(64),
            cell_preds_pos_buf: Vec::with_capacity(8),
            width,
            height,
            config: cfg,
        };

        sim.spawn_entities(initial_count, EntityType::Prey);
        sim.spawn_entities(15, EntityType::Predator);
        sim
    }

    fn spawn_entities(&mut self, count: usize, e_type: EntityType) {
        let mut rng = rand::thread_rng();
        let cfg = self.config;
        for _ in 0..count {
            let boid = Boid {
                position: Vector2D {
                    x: rng.gen_range(0.0..self.width),
                    y: rng.gen_range(0.0..self.height),
                },
                velocity: Vector2D {
                    x: rng.gen_range(-2.0..2.0),
                    y: rng.gen_range(-2.0..2.0),
                },
                e_type,
                is_alive: true,
                energy: if e_type == EntityType::Predator {
                    cfg.pred_init_energy
                } else {
                    10.0
                },
            };
            let idx = self.arena.insert(boid);
            self.active_indices.push(idx);
        }
    }

    pub fn prey_count(&self) -> usize {
        self.active_indices
            .iter()
            .filter(|&&idx| {
                self.arena
                    .get(idx)
                    .is_some_and(|b| b.e_type == EntityType::Prey)
            })
            .count()
    }

    pub fn predator_count(&self) -> usize {
        self.active_indices
            .iter()
            .filter(|&&idx| {
                self.arena
                    .get(idx)
                    .is_some_and(|b| b.e_type == EntityType::Predator)
            })
            .count()
    }

    pub fn tick(&mut self, ctx: &CanvasRenderingContext2d) {
        let cfg = self.config;
        let cols = (self.width / cfg.grid_cell_size).ceil() as usize + 1;
        let rows = (self.height / cfg.grid_cell_size).ceil() as usize + 1;

        for cell in &mut self.grid_cells {
            cell.clear();
        }
        self.predators_buf.clear();

        let mut current_prey_count = 0;
        let mut current_pred_count = 0;

        for &idx in &self.active_indices {
            if let Some(boid) = self.arena.get(idx) {
                if !boid.is_alive {
                    continue;
                }
                let cx = ((boid.position.x / cfg.grid_cell_size) as usize).min(cols - 1);
                let cy = ((boid.position.y / cfg.grid_cell_size) as usize).min(rows - 1);
                self.grid_cells[cy * cols + cx].push(idx);

                if boid.e_type == EntityType::Predator {
                    self.predators_buf.push(idx);
                    current_pred_count += 1;
                } else {
                    current_prey_count += 1;
                }
            }
        }

        // --- Deus Ex Machina ---
        let mut rng = rand::thread_rng();
        if current_prey_count < 100 && rng.gen_bool(0.05) {
            self.spawn_entities(10, EntityType::Prey);
        }
        if current_pred_count < 2 && rng.gen_bool(0.01) {
            self.spawn_entities(1, EntityType::Predator);
        }

        // --- B. Catch & Trace ---
        for i in 0..self.predators_buf.len() {
            let pred_idx = self.predators_buf[i];
            let mut prey_to_kill = None;
            let mut closest_prey_pos = None;
            let mut min_dist_sq = f32::MAX;

            if let Some(predator) = self.arena.get(pred_idx) {
                let cx = (predator.position.x / cfg.grid_cell_size) as isize;
                let cy = (predator.position.y / cfg.grid_cell_size) as isize;

                'outer: for dy in -1..=1 {
                    for dx in -1..=1 {
                        let nx = cx + dx;
                        let ny = cy + dy;
                        if nx >= 0 && nx < cols as isize && ny >= 0 && ny < rows as isize {
                            let cell_idx = (ny as usize) * cols + (nx as usize);
                            for &prey_idx in &self.grid_cells[cell_idx] {
                                if pred_idx.index == prey_idx.index {
                                    continue;
                                }
                                if let Some(prey) = self.arena.get(prey_idx) {
                                    if prey.e_type == EntityType::Prey && prey.is_alive {
                                        let dist_sq = predator.position.distance_sq(&prey.position);
                                        if dist_sq < 80.0 {
                                            prey_to_kill = Some(prey_idx);
                                            break 'outer;
                                        } else if dist_sq < 22500.0 && dist_sq < min_dist_sq {
                                            min_dist_sq = dist_sq;
                                            closest_prey_pos = Some(prey.position);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(prey_idx) = prey_to_kill {
                align!(self.arena, pred_idx, prey_idx, |pred, prey| {
                    prey.is_alive = false;
                    pred.energy += cfg.pred_heal_amount;
                });
            } else if let Some(target_pos) = closest_prey_pos {
                if let Some(pred) = self.arena.get_mut(pred_idx) {
                    let dx = target_pos.x - pred.position.x;
                    let dy = target_pos.y - pred.position.y;
                    pred.velocity.x += dx * 0.003;
                    pred.velocity.y += dy * 0.003;

                    let speed_sq =
                        pred.velocity.x * pred.velocity.x + pred.velocity.y * pred.velocity.y;
                    let max_sq = cfg.pred_max_speed * cfg.pred_max_speed;
                    if speed_sq > max_sq {
                        let speed = speed_sq.sqrt();
                        pred.velocity.x = (pred.velocity.x / speed) * cfg.pred_max_speed;
                        pred.velocity.y = (pred.velocity.y / speed) * cfg.pred_max_speed;
                    }
                }
            }
        }

        // --- C. Batch Update ---
        for cell in &self.grid_cells {
            if cell.len() < 2 {
                continue;
            }

            self.prey_neighbors_buf.clear();
            self.cell_preds_pos_buf.clear();

            let mut center_x = 0.0;
            let mut center_y = 0.0;
            let mut avg_vx = 0.0;
            let mut avg_vy = 0.0;

            for &idx in cell {
                if let Some(b) = self.arena.get(idx) {
                    if !b.is_alive {
                        continue;
                    }
                    if b.e_type == EntityType::Prey {
                        self.prey_neighbors_buf.push(idx);
                        center_x += b.position.x;
                        center_y += b.position.y;
                        avg_vx += b.velocity.x;
                        avg_vy += b.velocity.y;
                    } else {
                        self.cell_preds_pos_buf.push(b.position);
                    }
                }
            }

            let count = self.prey_neighbors_buf.len() as f32;
            if count > 0.0 {
                center_x /= count;
                center_y /= count;
                avg_vx /= count;
                avg_vy /= count;

                self.batch_buffer.clear_and_verify(&self.prey_neighbors_buf);

                self.arena.ordex(&self.batch_buffer, |mut iter| {
                    while let Some(boid) = iter.next() {
                        for p_pos in &self.cell_preds_pos_buf {
                            let dx = boid.position.x - p_pos.x;
                            let dy = boid.position.y - p_pos.y;
                            let dist_sq = dx * dx + dy * dy;
                            if dist_sq < 3000.0 {
                                boid.velocity.x += dx * 0.03;
                                boid.velocity.y += dy * 0.03;
                            }
                        }

                        let dx = center_x - boid.position.x;
                        let dy = center_y - boid.position.y;
                        let dist_sq = dx * dx + dy * dy;

                        if dist_sq < 250.0 {
                            boid.velocity.x -= dx * 0.03;
                            boid.velocity.y -= dy * 0.03;
                        } else {
                            boid.velocity.x += dx * 0.005;
                            boid.velocity.y += dy * 0.005;
                        }
                        boid.velocity.x += (avg_vx - boid.velocity.x) * 0.05;
                        boid.velocity.y += (avg_vy - boid.velocity.y) * 0.05;

                        let speed_sq =
                            boid.velocity.x * boid.velocity.x + boid.velocity.y * boid.velocity.y;
                        let max_sq = cfg.prey_max_speed * cfg.prey_max_speed;
                        let min_sq = cfg.prey_min_speed * cfg.prey_min_speed;
                        if speed_sq > max_sq {
                            let speed = speed_sq.sqrt();
                            boid.velocity.x = (boid.velocity.x / speed) * cfg.prey_max_speed;
                            boid.velocity.y = (boid.velocity.y / speed) * cfg.prey_max_speed;
                        } else if speed_sq < min_sq {
                            let speed = speed_sq.sqrt().max(0.001);
                            boid.velocity.x = (boid.velocity.x / speed) * cfg.prey_min_speed;
                            boid.velocity.y = (boid.velocity.y / speed) * cfg.prey_min_speed;
                        }
                    }
                });
            }
        }

        // --- D. Update ---
        let w = self.width;
        let h = self.height;
        let mut i = 0;
        let mut newborn_boids = Vec::new();

        while i < self.active_indices.len() {
            let idx = self.active_indices[i];
            let mut is_dead = false;

            if let Some(boid) = self.arena.get_mut(idx) {
                if !boid.is_alive {
                    is_dead = true;
                } else {
                    if boid.e_type == EntityType::Predator {
                        boid.energy -= cfg.pred_energy_drain;
                        if boid.energy <= 0.0 {
                            is_dead = true;
                        } else if boid.energy > cfg.pred_reproduce_energy
                            && current_pred_count < cfg.max_predators
                        {
                            boid.energy *= 0.5;
                            newborn_boids.push(Boid {
                                position: boid.position,
                                velocity: Vector2D {
                                    x: -boid.velocity.x,
                                    y: -boid.velocity.y,
                                },
                                e_type: EntityType::Predator,
                                is_alive: true,
                                energy: boid.energy,
                            });
                            current_pred_count += 1;
                        }
                    } else {
                        if rng.gen_bool(cfg.prey_reproduce_rate)
                            && current_prey_count < cfg.max_preys
                        {
                            newborn_boids.push(Boid {
                                position: boid.position,
                                velocity: Vector2D {
                                    x: -boid.velocity.x,
                                    y: -boid.velocity.y,
                                },
                                e_type: EntityType::Prey,
                                is_alive: true,
                                energy: 10.0,
                            });
                            current_prey_count += 1;
                        }
                    }

                    if !is_dead {
                        boid.position.x += boid.velocity.x;
                        boid.position.y += boid.velocity.y;
                        if boid.position.x < 0.0 {
                            boid.position.x = w;
                        } else if boid.position.x > w {
                            boid.position.x = 0.0;
                        }
                        if boid.position.y < 0.0 {
                            boid.position.y = h;
                        } else if boid.position.y > h {
                            boid.position.y = 0.0;
                        }
                    }
                }
            } else {
                is_dead = true;
            }

            if is_dead {
                self.arena.remove(idx);
                self.active_indices.swap_remove(i);
            } else {
                i += 1;
            }
        }

        for newborn in newborn_boids {
            let new_idx = self.arena.insert(newborn);
            self.active_indices.push(new_idx);
        }

        // --- E. Draw ---
        let _ = ctx.set_global_composite_operation("source-over");
        ctx.set_fill_style_str("#0a0a0a");
        ctx.fill_rect(0.0, 0.0, w as f64, h as f64);

        // --- Nodes (Prey) ---
        let preys: Vec<_> = self
            .active_indices
            .iter()
            .filter_map(|&idx| self.arena.get(idx).filter(|b| b.e_type == EntityType::Prey))
            .collect();

        let cell_size = 25.0;
        let draw_cols = (w as f64 / cell_size).ceil() as usize + 1;
        let draw_rows = (h as f64 / cell_size).ceil() as usize + 1;
        let mut draw_grid: Vec<Vec<usize>> = vec![Vec::new(); draw_cols * draw_rows];

        for (i, p) in preys.iter().enumerate() {
            let cx = ((p.position.x as f64 / cell_size).max(0.0) as usize).min(draw_cols - 1);
            let cy = ((p.position.y as f64 / cell_size).max(0.0) as usize).min(draw_rows - 1);
            draw_grid[cy * draw_cols + cx].push(i);
        }

        let colors = [
            "rgba(170, 150, 190, 0.9)",
            "rgba(200, 140, 140, 0.9)",
            "rgba(140, 160, 190, 0.9)",
            "rgba(140, 180, 150, 0.9)",
        ];

        ctx.set_line_width(0.35);

        for (color_idx, &color) in colors.iter().enumerate() {
            ctx.set_stroke_style_str(color);
            ctx.begin_path();

            for i in 0..preys.len() {
                if i % 4 != color_idx {
                    continue;
                }

                let p1 = preys[i];
                let px1 = p1.position.x as f64;
                let py1 = p1.position.y as f64;
                let cx = ((px1 / cell_size).max(0.0) as usize).min(draw_cols - 1);
                let cy = ((py1 / cell_size).max(0.0) as usize).min(draw_rows - 1);

                ctx.move_to(px1, py1);
                ctx.line_to(px1 + 0.5, py1);

                let mut connections = 0;

                'search: for dy in -1..=1 {
                    for dx in -1..=1 {
                        let nx = cx as isize + dx;
                        let ny = cy as isize + dy;

                        if nx >= 0 && nx < draw_cols as isize && ny >= 0 && ny < draw_rows as isize
                        {
                            let cell_idx = (ny as usize) * draw_cols + (nx as usize);

                            for &j in &draw_grid[cell_idx] {
                                if j <= i {
                                    continue;
                                }

                                let p2 = preys[j];
                                let d_x = px1 - p2.position.x as f64;
                                let d_y = py1 - p2.position.y as f64;
                                let dist_sq = d_x * d_x + d_y * d_y;

                                if dist_sq < 500.0 {
                                    ctx.move_to(px1, py1);
                                    ctx.line_to(p2.position.x as f64, p2.position.y as f64);
                                    connections += 1;
                                    if connections >= 5 {
                                        break 'search;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            ctx.stroke();
        }

        // --- Agents (Predator) ---
        ctx.set_stroke_style_str("#0A0A0A");
        ctx.set_line_width(1.0);
        ctx.begin_path();

        for &idx in &self.active_indices {
            if let Some(boid) = self.arena.get(idx) {
                if boid.e_type == EntityType::Predator {
                    let px = boid.position.x as f64;
                    let py = boid.position.y as f64;

                    ctx.move_to(px, py);
                    ctx.line_to(px + 1.0, py);
                }
            }
        }
        ctx.stroke();
    }
}
