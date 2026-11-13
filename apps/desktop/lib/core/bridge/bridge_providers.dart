import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'api.dart';
import 'frb_generated.dart';

/// Lazy, one-shot initialization of the Rust bridge.
///
/// Kept out of `main()` so widget tests never touch real FFI: the dynamic
/// library is only loaded when a provider that needs it is watched, and
/// tests override those providers (test/widget_test.dart).
Future<void> _ensureBridgeInitialized() {
  _initFuture ??= RustLib.init();
  return _initFuture!;
}

Future<void>? _initFuture;

/// Host system facts from the Rust engine (os/arch/capabilities).
///
/// Used for platform gating (DESIGN.md §7) and the diagnostics screen.
final systemInfoProvider = FutureProvider<SystemInfo>((ref) async {
  await _ensureBridgeInitialized();
  return getSystemInfo();
});

/// Engine identity (version/name) for the About screen and diagnostics.
final appInfoProvider = FutureProvider<AppInfo>((ref) async {
  await _ensureBridgeInitialized();
  return getAppInfo();
});
