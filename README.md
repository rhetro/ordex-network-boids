# Ordex Network Boids Simulation

![Live Demo](https://rhetro.pages.dev/rust/ordex/)

A WebAssembly-powered continuous Lotka-Volterra predator-prey Boids simulation capable of managing over 3,500 active entities at high frame rates (display VSync-capped, e.g., 60–120+ FPS).

This project serves as a real-world proof of concept (PoC) for [Ordex](https://github.com/rhetro/ordex), demonstrating concurrent mutable references and dynamic aliasing verification without runtime borrow checking overhead.

## Key Features

* **3,500+ Active Entities**: High-density, real-time simulation of interacting prey and predator boids via Wasm.
* **Zero-Allocation Batch Verification**: Reuses Ordex internal workspace buffers via `clear_and_verify` to prevent heap allocations during multi-mutable access verification in simulation loops.
* **Disjoint Access Enforcement**: Leverages `OrdexArena`'s static (`align!`) and dynamic (`ordex`) validation to safely bypass standard borrow checking bottlenecks.
* **Spatial Hashing Integration**: Efficient cell-based neighborhood search mapped directly into Ordex's hybrid SoA bucket routing engine.
* **Batched Canvas2D Rendering**: Minimizes WebSys binding overhead by batching path commands per frame, maintaining smooth performance on desktop and mobile displays.

## Architecture & Approach

Standard Rust borrow rules prevent simultaneous mutable access to multiple disparate elements in non-linear data structures. This simulation resolves two distinct forms of concurrent access:

1. **Targeted Interaction (`align!`)**:
   Used when predators capture prey. Resolves simultaneous mutable references between two distinct entities (`predator` and `prey`) via unrolled stack checking.
2. **Batch Spatial Updates (`ordex`)**:
   Used during neighborhood Boids updates. Resolves dynamic clusters of neighboring entities per frame through hybrid SoA bucketing and bitwise verification, reusing verification buffers to avoid per-frame heap allocations.

## Prerequisites

* Rust (2021 edition)
* `wasm-pack` (`cargo install wasm-pack`)

## Build & Run

### 1. Build WebAssembly Package

```bash
wasm-pack build --target web --release
```

### 2. Start Local Web Server

Run a local HTTP server in the repository root directory:

```bash
# Using Python
python -m http.server 8080

# Or using Node.js
npx http-server . -c-1
```

### 3. Open in Browser

Navigate to `http://localhost:8080` to view the live simulation.

## License

This project is licensed under either of

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))
* MIT license ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))

at your option.
