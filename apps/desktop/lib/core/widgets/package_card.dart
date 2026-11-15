import 'package:material_ui/material_ui.dart';

import '../../app/theme/design_tokens.dart';
import '../domain/domain_models.dart';
import 'package_monogram.dart';

/// Package card for the Explore list (DESIGN.md §4, §7): name, short
/// description, publisher, category, and supported-platform indicators.
class PackageCard extends StatelessWidget {
  const PackageCard({
    super.key,
    required this.package,
    required this.onOpen,
    this.installedVersion,
  });

  final CatalogPackage package;
  final VoidCallback onOpen;

  /// Shown as a status line when the package is already installed.
  final String? installedVersion;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final supportsCurrent = package.supportsPlatform(PlatformTarget.current);

    return Card(
      elevation: 0,
      margin: const EdgeInsets.only(bottom: DesignTokens.space3),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: theme.colorScheme.outlineVariant),
      ),
      child: InkWell(
        borderRadius: BorderRadius.circular(12),
        onTap: onOpen,
        child: Padding(
          padding: const EdgeInsets.all(DesignTokens.space4),
          child: Row(
            children: [
              // Brand monogram (user feedback: cards recognizable by a
              // logo-like badge, e.g. the Go card).
              PackageMonogram(package: package),
              const SizedBox(width: DesignTokens.space3),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Flexible(
                          child: Text(
                            package.name,
                            style: theme.textTheme.titleMedium,
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                        const SizedBox(width: DesignTokens.space2),
                        _CategoryBadge(category: package.category),
                      ],
                    ),
                    const SizedBox(height: DesignTokens.space1),
                    Text(
                      package.description,
                      style: theme.textTheme.bodyMedium?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                    ),
                    const SizedBox(height: DesignTokens.space2),
                    Row(
                      children: [
                        Flexible(
                          child: Text(
                            package.publisher,
                            style: theme.textTheme.bodySmall,
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                        const SizedBox(width: DesignTokens.space3),
                        if (installedVersion != null) ...[
                          Icon(
                            Icons.check_circle_outline,
                            size: 16,
                            color: theme.colorScheme.primary,
                          ),
                          const SizedBox(width: DesignTokens.space1),
                          Text(
                            'Installed $installedVersion',
                            style: theme.textTheme.bodySmall?.copyWith(
                              color: theme.colorScheme.primary,
                            ),
                          ),
                        ],
                      ],
                    ),
                  ],
                ),
              ),
              const SizedBox(width: DesignTokens.space3),
              PlatformCompatibility(
                platforms: package.platforms,
                compact: true,
                highlightUnsupported: !supportsCurrent,
              ),
              const SizedBox(width: DesignTokens.space2),
              Icon(
                Icons.chevron_right,
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Small category label used inside cards: icon + label, so the category
/// is recognizable by icon as well as text.
class _CategoryBadge extends StatelessWidget {
  const _CategoryBadge({required this.category});

  final PackageCategory category;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: DesignTokens.space2,
        vertical: 2,
      ),
      decoration: BoxDecoration(
        color: theme.colorScheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(999),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(category.icon, size: 12, color: theme.colorScheme.primary),
          const SizedBox(width: 4),
          Text(category.label, style: theme.textTheme.labelSmall),
        ],
      ),
    );
  }
}

/// Icon per catalog category (user feedback: categories must be
/// recognizable by icon, not only by text label). All icons are standard
/// Material symbols so they exist on every Flutter version we pin.
extension PackageCategoryIcon on PackageCategory {
  IconData get icon => switch (this) {
    PackageCategory.language => Icons.terminal,
    PackageCategory.framework => Icons.widgets_outlined,
    PackageCategory.sdk => Icons.inventory_2_outlined,
    PackageCategory.ide => Icons.desktop_windows_outlined,
    PackageCategory.buildTool => Icons.construction,
    PackageCategory.database => Icons.storage,
    PackageCategory.utility => Icons.handyman_outlined,
    PackageCategory.aiTool => Icons.auto_awesome,
    PackageCategory.other => Icons.category_outlined,
  };
}

/// Category filter chip (DESIGN.md §4 Explore): icon + label.
class CategoryChip extends StatelessWidget {
  const CategoryChip({
    super.key,
    required this.category,
    required this.selected,
    this.onSelected,
  });

  final PackageCategory category;
  final bool selected;
  final VoidCallback? onSelected;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(right: DesignTokens.space2),
      child: FilterChip(
        avatar: Icon(category.icon, size: 18),
        label: Text(category.label),
        selected: selected,
        onSelected: onSelected == null ? null : (_) => onSelected!(),
        showCheckmark: false,
      ),
    );
  }
}

/// Supported-platform indicators (DESIGN.md §4/§7). Highlights platforms
/// that are NOT this build's platform without relying on color alone
/// (labels show OS/arch).
class PlatformCompatibility extends StatelessWidget {
  const PlatformCompatibility({
    super.key,
    required this.platforms,
    this.compact = false,
    this.highlightUnsupported = false,
  });

  final List<PlatformTarget> platforms;
  final bool compact;
  final bool highlightUnsupported;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final shown = compact ? platforms.take(3).toList() : platforms;
    return Wrap(
      spacing: DesignTokens.space1,
      runSpacing: DesignTokens.space1,
      children: [
        for (final platform in shown)
          Tooltip(
            message: '${platform.os} ${platform.arch}',
            child: Container(
              padding: const EdgeInsets.symmetric(
                horizontal: DesignTokens.space2,
                vertical: 2,
              ),
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(6),
                border: Border.all(color: theme.colorScheme.outlineVariant),
              ),
              child: Text(
                compact
                    ? '${platform.os.substring(0, 3)}'
                          '${platform.arch == 'x64' ? '' : '·arm'}'
                    : '${platform.os} ${platform.arch}',
                style: theme.textTheme.labelSmall,
              ),
            ),
          ),
        if (compact && platforms.length > shown.length)
          Text(
            '+${platforms.length - shown.length}',
            style: theme.textTheme.labelSmall,
          ),
      ],
    );
  }
}

/// Source-verification status (DESIGN.md §4: "Never label a package as
/// trusted merely because it appears in the catalog"). Phase 1 shows the
/// accurate status: the manifest is catalog data; source signatures are
/// verified at download time, not before.
class TrustStatus extends StatelessWidget {
  const TrustStatus({super.key, required this.sourceDomain});

  final String sourceDomain;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      children: [
        Icon(
          Icons.fact_check_outlined,
          size: 18,
          color: theme.colorScheme.onSurfaceVariant,
        ),
        const SizedBox(width: DesignTokens.space2),
        Expanded(
          child: Text(
            'Source: $sourceDomain · checksum recorded, verified at download '
            'time · publisher signature not yet checked',
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ),
      ],
    );
  }
}
