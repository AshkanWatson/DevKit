import 'package:material_ui/material_ui.dart';

import 'design_tokens.dart';

/// Application theme (DESIGN.md §5–§7).
///
/// Uses semantic tokens for both modes. Typography follows DESIGN.md §6:
/// a legible sans-serif with platform fallback and tabular numerals for
/// version, size, and progress values.
abstract final class AppTheme {
  static const _fontFamilyFallback = <String>[
    'Segoe UI Variable Display',
    'Segoe UI',
    'SF Pro Text',
    'Roboto',
  ];

  static ThemeData light() => _theme(_lightScheme, _lightExtensions);

  static ThemeData dark() => _theme(_darkScheme, _darkExtensions);

  static const _lightScheme = ColorScheme.light(
    primary: DesignTokens.accentLight,
    onPrimary: Colors.white,
    surface: DesignTokens.surfaceLight,
    onSurface: DesignTokens.textPrimaryLight,
    onSurfaceVariant: DesignTokens.textSecondaryLight,
    surfaceContainerHighest: DesignTokens.surfaceElevatedLight,
    outline: DesignTokens.borderLight,
    error: DesignTokens.dangerLight,
  );

  static const _darkScheme = ColorScheme.dark(
    primary: DesignTokens.accentDark,
    onPrimary: Color(0xFF10131A),
    surface: DesignTokens.surfaceDark,
    onSurface: DesignTokens.textPrimaryDark,
    onSurfaceVariant: DesignTokens.textSecondaryDark,
    surfaceContainerHighest: DesignTokens.surfaceElevatedDark,
    outline: DesignTokens.borderDark,
    error: DesignTokens.dangerDark,
  );

  static const _lightExtensions = <ThemeExtension<dynamic>>[];
  static const _darkExtensions = <ThemeExtension<dynamic>>[];

  static ThemeData _theme(
    ColorScheme scheme,
    List<ThemeExtension<dynamic>> extensions,
  ) {
    final base = ThemeData(
      useMaterial3: true,
      colorScheme: scheme,
      fontFamilyFallback: _fontFamilyFallback,
      scaffoldBackgroundColor: scheme.surface,
      extensions: extensions,
    );

    final body = base.textTheme.bodyMedium!.copyWith(height: 1.45);

    // Type scale (DESIGN.md §6): page title, section title, card title,
    // body, metadata, caption. Tabular numerals for numeric metadata.
    const tabularFigures = <FontFeature>[FontFeature.tabularFigures()];
    final textTheme = base.textTheme.copyWith(
      headlineSmall: base.textTheme.headlineSmall!.copyWith(
        fontWeight: FontWeight.w600,
      ),
      titleLarge: base.textTheme.titleLarge!.copyWith(
        fontWeight: FontWeight.w600,
      ),
      titleMedium: base.textTheme.titleMedium!.copyWith(
        fontWeight: FontWeight.w600,
      ),
      bodyMedium: body,
      bodySmall: base.textTheme.bodySmall!.copyWith(
        color: scheme.onSurfaceVariant,
      ),
      labelSmall: base.textTheme.labelSmall!.copyWith(
        color: scheme.onSurfaceVariant,
        fontFeatures: tabularFigures,
      ),
    );

    return base.copyWith(
      textTheme: textTheme,
      dividerTheme: DividerThemeData(
        color: scheme.outline,
        thickness: 1,
        space: 1,
      ),
      navigationRailTheme: NavigationRailThemeData(
        backgroundColor: scheme.surface,
        indicatorColor: scheme.primary.withValues(alpha: 0.12),
        selectedIconTheme: IconThemeData(color: scheme.primary),
        selectedLabelTextStyle: textTheme.labelLarge!.copyWith(
          color: scheme.primary,
        ),
        unselectedIconTheme: IconThemeData(color: scheme.onSurfaceVariant),
        unselectedLabelTextStyle: textTheme.labelLarge!.copyWith(
          color: scheme.onSurfaceVariant,
        ),
      ),
      cardTheme: CardThemeData(
        color: scheme.surface,
        elevation: 0,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(10),
          side: BorderSide(color: scheme.outline),
        ),
        margin: EdgeInsets.zero,
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: scheme.surfaceContainerHighest,
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(8),
          borderSide: BorderSide(color: scheme.outline),
        ),
        enabledBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(8),
          borderSide: BorderSide(color: scheme.outline),
        ),
        focusedBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(8),
          borderSide: BorderSide(color: scheme.primary, width: 2),
        ),
        contentPadding: const EdgeInsets.symmetric(
          horizontal: DesignTokens.space3,
          vertical: DesignTokens.space3,
        ),
      ),
      filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
          padding: const EdgeInsets.symmetric(
            horizontal: DesignTokens.space4,
            vertical: DesignTokens.space3,
          ),
        ),
      ),
      outlinedButtonTheme: OutlinedButtonThemeData(
        style: OutlinedButton.styleFrom(
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
          side: BorderSide(color: scheme.outline),
          padding: const EdgeInsets.symmetric(
            horizontal: DesignTokens.space4,
            vertical: DesignTokens.space3,
          ),
        ),
      ),
    );
  }
}
