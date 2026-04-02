# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## `Unreleased`

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
