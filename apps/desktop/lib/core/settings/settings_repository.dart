import 'package:shared_preferences/shared_preferences.dart';

/// Key-value persistence for UI-level settings (theme mode, P1-07).
///
/// Deliberately narrow: all DevKit state — catalog cache, installed
/// registry, job history — lives in the Rust-managed SQLite database
/// (docs/architecture/local-database.md). Nothing sensitive is ever
/// written here (docs/security/security-model.md).
abstract interface class SettingsRepository {
  Future<String?> read(String key);

  Future<void> write(String key, String value);
}

/// Default implementation over the shared_preferences plugin.
class SharedPreferencesSettings implements SettingsRepository {
  @override
  Future<String?> read(String key) async {
    final prefs = await SharedPreferences.getInstance();
    return prefs.getString(key);
  }

  @override
  Future<void> write(String key, String value) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(key, value);
  }
}
