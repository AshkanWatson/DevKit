import 'package:material_ui/material_ui.dart';

/// Design tokens (DESIGN.md §5).
///
/// These are the starting values from the design specification, exposed as
/// semantic tokens. Widgets must reference tokens, not raw color values.
/// Tokens are starting values, not immutable brand identity.
abstract final class DesignTokens {
  // Light theme.
  static const backgroundLight = Color(0xFFF7F8FA);
  static const surfaceLight = Color(0xFFFFFFFF);
  static const surfaceElevatedLight = Color(0xFFF0F2F5);
  static const textPrimaryLight = Color(0xFF17191C);
  static const textSecondaryLight = Color(0xFF626A73);
  static const borderLight = Color(0xFFE1E5E9);
  static const accentLight = Color(0xFF246BFD);
  static const successLight = Color(0xFF16845B);
  static const warningLight = Color(0xFFA96812);
  static const dangerLight = Color(0xFFC43D4B);

  // Dark theme.
  static const backgroundDark = Color(0xFF111315);
  static const surfaceDark = Color(0xFF1A1D20);
  static const surfaceElevatedDark = Color(0xFF24282C);
  static const textPrimaryDark = Color(0xFFF3F5F7);
  static const textSecondaryDark = Color(0xFFA4ADB7);
  static const borderDark = Color(0xFF30363D);
  static const accentDark = Color(0xFF6D9BFF);
  static const successDark = Color(0xFF39B980);
  static const warningDark = Color(0xFFE6AD55);
  static const dangerDark = Color(0xFFF0717D);

  // Spacing rhythm: 8-point grid with 4-point adjustments (DESIGN.md §6).
  static const space1 = 4.0;
  static const space2 = 8.0;
  static const space3 = 12.0;
  static const space4 = 16.0;
  static const space5 = 24.0;
  static const space6 = 32.0;
  static const space7 = 48.0;

  // Responsive breakpoints (DESIGN.md §9). Desktop-first.
  static const breakpointNarrow = 700.0;
  static const breakpointWide = 1100.0;

  static const sidebarWidth = 220.0;
  static const compactRailWidth = 72.0;
}
