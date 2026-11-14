import 'dart:io';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import 'settings_repository.dart';

/// Settings persistence source. Tests override this with an in-memory
/// repository (widget tests must never write real user preferences).
final settingsRepositoryProvider = Provider<SettingsRepository>(
  (ref) => SharedPreferencesSettings(),
);

/// Theme mode (DESIGN.md §4 Settings): system/light/dark, persisted
/// (P1-07 acceptance criterion). While the persisted value loads the app
/// follows the system, which is also the fallback for unknown values.
class ThemeModeController extends AsyncNotifier<ThemeMode> {
  static const _key = 'theme_mode';

  @override
  Future<ThemeMode> build() async {
    final raw = await ref.watch(settingsRepositoryProvider).read(_key);
    return ThemeMode.values.firstWhere(
      (mode) => mode.name == raw,
      orElse: () => ThemeMode.system,
    );
  }

  /// Applies immediately and persists; a failed write is logged by the
  /// repository and retried on the next start.
  Future<void> setMode(ThemeMode mode) async {
    state = AsyncData(mode);
    await ref.read(settingsRepositoryProvider).write(_key, mode.name);
  }
}

final themeModeProvider = AsyncNotifierProvider<ThemeModeController, ThemeMode>(
  ThemeModeController.new,
);

/// The DevKit data root shown on Settings (local-database.md §1): the
/// OS-standard user data location — paths are never hard-coded in the
/// engine. Phase 1 resolves the Windows default locally; the Rust engine
/// owns the authoritative resolution once the settings FFI lands, and
/// this provider is the single place to swap.
final storageLocationProvider = Provider<String>((ref) {
  final appData =
      Platform.environment['APPDATA'] ?? Platform.environment['HOME'];
  final base = appData ?? '~';
  return '$base\\DevKit\\state\\devkit.sqlite';
});
