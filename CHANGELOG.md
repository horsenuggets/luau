# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## `0.742-horse.1.3`

### Added

- Added the Cargo-compatible `luau0-src` build interface over this repository's native
  sources, with real `mlua` consumer tests for coverage, vectors, callbacks and codegen.

### Fixed

- Traverse actual bytecode instructions when collecting coverage so AUX operands cannot
  become phantom executable lines or hit counts. Preserve coverage under breakpoints.
- Avoid nullable codegen logging calls and unnecessary type-function argument copies
  that prevented warning-clean optimized GCC builds.
- Return a boolean from isocline's buffered input path to keep Windows CLI builds
  warning-clean.
- Match flags-enabled golden diagnostics to the existing terse type-function chunk names.
- Exclude unrelated formatter submodules from Cargo's recursive Git fetch so native
  source consumers do not need SSH credentials.

## `0.742-horse.1.2`

### Added

- `UnreachableCode` lint warns about statements after a call to a function whose return type is `never` (both old and new type solvers)

### Changed

- Based on upstream Luau 0.742

## `0.710-horse.1.1`

### Fixed

- Fix `typeof(require())` type propagation to work across module boundaries (both old and new type solvers)
- Remove invalid `debug.getcoverage` type definition (C API function, not a Luau debug library function)

## `0.710-horse.1.0`

### Added

- Propagate export types through `typeof(require())` redirects
- Pass a `lua_State` parameter to `useratom`
- Allow two-argument explicit constructor for `RequireAlias`
- Explicit type instantiations (`f<<T>>`)
- `math.isnan`, `math.isinf`, and `math.isfinite` implementation
- Expose `luaL_traceback` from Lua C API

### Changed

- Based on upstream Luau 0.710
