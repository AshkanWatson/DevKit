import 'package:material_ui/material_ui.dart';

import '../domain/domain_models.dart';

/// Brand monogram for package cards (user feedback 2026-11-14: the Explore
/// cards must be recognizable by a logo-like badge, e.g. the Go card).
///
/// We deliberately do **not** bundle vendor logo artwork: logos are
/// trademarks with per-vendor usage rules. A colored monogram in the
/// vendor's brand color is recognizable, license-safe, and consistent
/// with the Material card design. The mapping lives in the widget layer —
/// domain models stay pure Dart.
abstract final class PackageMonograms {
  /// Monogram letters and brand color per stable package id.
  static const _brands = <String, (String, Color)>{
    // Languages
    'devkit.lang.nodejs': ('JS', Color(0xFF5FA04E)),
    'devkit.lang.go': ('GO', Color(0xFF00ADD8)),
    'devkit.lang.rust': ('RS', Color(0xFFF74C00)),
    'devkit.lang.java': ('J', Color(0xFFF89820)),
    'devkit.lang.kotlin': ('K', Color(0xFF7F52FF)),
    'devkit.lang.python': ('PY', Color(0xFF3776AB)),
    // Frameworks
    'devkit.framework.flutter': ('F', Color(0xFF02569B)),
    'devkit.framework.spring-boot': ('S', Color(0xFF6DB33F)),
    'devkit.framework.nextjs': ('N', Color(0xFF1F1F1F)),
    // SDKs
    'devkit.sdk.dotnet': ('NET', Color(0xFF512BD4)),
    'devkit.sdk.android-tools': ('A', Color(0xFF3DDC84)),
    // IDEs
    'devkit.ide.android-studio': ('AS', Color(0xFF00BFA5)),
    'devkit.ide.intellij': ('IJ', Color(0xFFFE315D)),
    'devkit.ide.vscode': ('VS', Color(0xFF007ACC)),
    // Build tools
    'devkit.build.gradle': ('GR', Color(0xFF02303A)),
    'devkit.build.maven': ('M', Color(0xFFC71A36)),
    'devkit.build.cmake': ('C', Color(0xFFD33833)),
    // Databases
    'devkit.database.postgresql': ('PG', Color(0xFF336791)),
    'devkit.database.redis': ('RE', Color(0xFFDC382D)),
    'devkit.database.sqlite': ('SQ', Color(0xFF0F80CC)),
    // AI tools
    'devkit.ai.ollama': ('O', Color(0xFF4B4B4B)),
    // Utilities
    'devkit.utility.git': ('G', Color(0xFFF05032)),
    'devkit.utility.docker': ('D', Color(0xFF2496ED)),
  };

  /// Brand color for a package id, or null when the catalog carries a
  /// package without a monogram (fallback: category badge).
  static Color? brandColor(String packageId) => _brands[packageId]?.$2;

  /// Monogram letters for a package id, or null when unmapped.
  static String? monogram(String packageId) => _brands[packageId]?.$1;
}

/// The badge itself: rounded square in the brand color with the monogram.
/// Falls back to the category color/icon when the package is unmapped.
class PackageMonogram extends StatelessWidget {
  const PackageMonogram({super.key, required this.package, this.size = 40});

  final CatalogPackage package;
  final double size;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final brand = PackageMonograms.monogram(package.id);
    final color = PackageMonograms.brandColor(package.id);
    // Unmapped packages (future catalog entries): a neutral badge.
    final background = color ?? theme.colorScheme.surfaceContainerHighest;
    final foreground = color != null
        ? Colors.white
        : theme.colorScheme.onSurfaceVariant;

    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        color: background,
        borderRadius: BorderRadius.circular(10),
      ),
      alignment: Alignment.center,
      child: brand == null
          ? Icon(Icons.category_outlined, size: size * 0.5, color: foreground)
          : Text(
              brand,
              style: theme.textTheme.labelLarge?.copyWith(
                color: foreground,
                fontWeight: FontWeight.w700,
                letterSpacing: 0.2,
              ),
            ),
    );
  }
}
