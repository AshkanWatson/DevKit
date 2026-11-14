import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';

import '../../app/theme/design_tokens.dart';
import '../../core/domain/domain_models.dart';
import '../../core/providers/repository_providers.dart';
import '../../core/widgets/common_widgets.dart';
import '../../core/widgets/empty_state.dart';
import '../../core/widgets/global_search.dart';
import '../../core/widgets/package_card.dart';

/// Explore destination (DESIGN.md §4): prominent search, category filter
/// chips, and package cards. Search query and category survive navigating
/// away and back (StatefulShellRoute keeps branch state).
class ExploreScreen extends ConsumerWidget {
  const ExploreScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final packages = ref.watch(catalogPackagesProvider);
    final query = ref.watch(_searchQueryProvider);
    final category = ref.watch(_categoryFilterProvider);

    return Scaffold(
      body: packages.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (error, _) => ErrorState(
          title: 'Catalog could not be loaded',
          error: error,
          onRetry: () => ref.invalidate(catalogPackagesProvider),
        ),
        data: (all) {
          final filtered = _applyFilters(all, query, category);
          return _ExploreBody(
            packages: all,
            filtered: filtered,
            query: query,
            category: category,
          );
        },
      ),
    );
  }
}

List<CatalogPackage> _applyFilters(
  List<CatalogPackage> packages,
  String query,
  PackageCategory? category,
) {
  final lowerQuery = query.trim().toLowerCase();
  return [
    for (final package in packages)
      if (category == null || package.category == category)
        if (lowerQuery.isEmpty ||
            package.name.toLowerCase().contains(lowerQuery) ||
            package.description.toLowerCase().contains(lowerQuery) ||
            package.publisher.toLowerCase().contains(lowerQuery) ||
            package.id.toLowerCase().contains(lowerQuery))
          package,
  ];
}

/// Search and filter state lives in providers so it is preserved across
/// navigation (DESIGN.md §8).
class _SearchQuery extends Notifier<String> {
  @override
  String build() => '';

  void set(String value) => state = value;
}

class _CategoryFilter extends Notifier<PackageCategory?> {
  @override
  PackageCategory? build() => null;

  void set(PackageCategory? value) => state = value;
}

final _searchQueryProvider = NotifierProvider<_SearchQuery, String>(
  _SearchQuery.new,
);
final _categoryFilterProvider =
    NotifierProvider<_CategoryFilter, PackageCategory?>(_CategoryFilter.new);

class _ExploreBody extends ConsumerWidget {
  const _ExploreBody({
    required this.packages,
    required this.filtered,
    required this.query,
    required this.category,
  });

  final List<CatalogPackage> packages;
  final List<CatalogPackage> filtered;
  final String query;
  final PackageCategory? category;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final categories = {
      for (final package in packages) package.category,
    }.toList()..sort((a, b) => a.label.compareTo(b.label));

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(
            DesignTokens.space5,
            DesignTokens.space5,
            DesignTokens.space5,
            DesignTokens.space3,
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('Explore', style: theme.textTheme.headlineSmall),
              const SizedBox(height: DesignTokens.space3),
              GlobalSearch(
                value: query,
                onChanged: (value) =>
                    ref.read(_searchQueryProvider.notifier).set(value),
              ),
              const SizedBox(height: DesignTokens.space3),
              SingleChildScrollView(
                scrollDirection: Axis.horizontal,
                child: Row(
                  children: [
                    CategoryChip(
                      label: 'All',
                      selected: category == null,
                      onSelected: () =>
                          ref.read(_categoryFilterProvider.notifier).set(null),
                    ),
                    for (final candidate in categories)
                      CategoryChip(
                        label: candidate.label,
                        selected: category == candidate,
                        onSelected: () => ref
                            .read(_categoryFilterProvider.notifier)
                            .set(candidate),
                      ),
                  ],
                ),
              ),
            ],
          ),
        ),
        Expanded(
          child: filtered.isEmpty
              ? EmptyState(
                  title: 'No packages match',
                  message:
                      'No catalog package matches the current search and '
                      'filters. Adjust the query or category to see more.',
                )
              : ListView.builder(
                  padding: const EdgeInsets.fromLTRB(
                    DesignTokens.space5,
                    0,
                    DesignTokens.space5,
                    DesignTokens.space5,
                  ),
                  itemCount: filtered.length,
                  itemBuilder: (context, index) {
                    final package = filtered[index];
                    return PackageCard(
                      package: package,
                      onOpen: () => context.go('/explore/${package.id}'),
                    );
                  },
                ),
        ),
      ],
    );
  }
}
