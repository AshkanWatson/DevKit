import 'dart:async';

import 'package:devkit/core/bridge/api.dart';
import 'package:devkit/core/bridge/bridge_providers.dart';
import 'package:devkit/core/domain/domain_models.dart';
import 'package:devkit/core/domain/repository_contracts.dart';
import 'package:devkit/core/data/fake_repositories.dart';
import 'package:devkit/core/providers/repository_providers.dart';
import 'package:devkit/core/widgets/common_widgets.dart';
import 'package:devkit/main.dart';
import 'package:fake_async/fake_async.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:riverpod/misc.dart';

/// P1-06 screen and component tests.
///
/// Rules honored here:
/// - Widget tests never touch real FFI: the bridge provider is overridden.
/// - The Downloads screen is never driven with `pumpAndSettle` while a job
///   is active (indeterminate progress animates forever); the seeded
///   fixture jobs used below are all terminal, so settling is safe.
/// - Job-simulator semantics (state machine, rollback, cancellation) are
///   tested without real timers via [fakeAsync] and the `debugAdvanceOnce`
///   seam.
void main() {
  const fakeSystemInfo = SystemInfo(
    os: 'windows',
    arch: 'x86_64',
    capabilities: ['sqlite_registry', 'catalog_client'],
  );

  Widget buildApp({List<Override> overrides = const []}) {
    return DevKitRoot(
      overrides: [
        systemInfoProvider.overrideWith((ref) => Future.value(fakeSystemInfo)),
        ...overrides,
      ],
    );
  }

  /// `FilledButton.icon` instantiates a subclass, so `widgetWithText`
  /// (exact-type matching) misses it. Match the button by subtype.
  Finder filledButtonWithLabel(String label) => find.ancestor(
    of: find.text(label),
    matching: find.byWidgetPredicate((widget) => widget is FilledButton),
  );

  /// The NavigationRail shows destination icons that also occur as body
  /// content on some screens; scope navigation taps to the rail.
  Future<void> goTo(WidgetTester tester, IconData icon) async {
    await tester.tap(
      find.descendant(
        of: find.byType(NavigationRail),
        matching: find.byIcon(icon),
      ),
    );
    await tester.pumpAndSettle();
  }

  group('Explore', () {
    testWidgets('search filters packages by name, description, publisher', (
      tester,
    ) async {
      await tester.pumpWidget(buildApp());
      await tester.pumpAndSettle();
      await goTo(tester, Icons.explore_outlined);

      await tester.enterText(find.byType(TextField), 'python');
      await tester.pumpAndSettle();

      expect(find.text('Python'), findsOneWidget);
      expect(find.text('Flutter'), findsNothing);
      expect(find.text('Docker Engine'), findsNothing);
    });

    testWidgets('category chips filter the list', (tester) async {
      await tester.pumpWidget(buildApp());
      await tester.pumpAndSettle();
      await goTo(tester, Icons.explore_outlined);

      await tester.tap(find.widgetWithText(FilterChip, 'Utilities'));
      await tester.pumpAndSettle();

      expect(find.text('Git'), findsOneWidget);
      expect(find.text('Docker Engine'), findsOneWidget);
      expect(find.text('Python'), findsNothing);

      // "All" restores the full catalog.
      await tester.tap(find.widgetWithText(FilterChip, 'All'));
      await tester.pumpAndSettle();
      expect(find.text('Python'), findsOneWidget);
    });

    testWidgets('search and category state survive navigation', (tester) async {
      await tester.pumpWidget(buildApp());
      await tester.pumpAndSettle();
      await goTo(tester, Icons.explore_outlined);
      await tester.enterText(find.byType(TextField), 'python');
      await tester.pumpAndSettle();

      // Leave Explore and come back: branch state is preserved
      // (P1-01 acceptance criterion).
      await goTo(tester, Icons.inventory_2_outlined);
      await goTo(tester, Icons.explore_outlined);

      expect(find.text('Python'), findsOneWidget);
      expect(find.text('Flutter'), findsNothing);
    });

    testWidgets('loading state shows a spinner, then the catalog', (
      tester,
    ) async {
      final delayed = _DelayedCatalog();
      await tester.pumpWidget(
        buildApp(
          overrides: [catalogRepositoryProvider.overrideWith((ref) => delayed)],
        ),
      );
      await tester.pump();
      await tester.tap(
        find.descendant(
          of: find.byType(NavigationRail),
          matching: find.byIcon(Icons.explore_outlined),
        ),
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));

      expect(find.byType(CircularProgressIndicator), findsOneWidget);

      await delayed.release();
      await tester.pumpAndSettle();
      expect(find.text('Python'), findsOneWidget);
    });

    testWidgets('catalog load failure shows an error state with retry', (
      tester,
    ) async {
      await tester.pumpWidget(
        buildApp(
          overrides: [
            catalogRepositoryProvider.overrideWith(
              (ref) => _ExplodingCatalogRepository(),
            ),
          ],
        ),
      );
      await tester.pumpAndSettle();
      await goTo(tester, Icons.explore_outlined);

      expect(find.text('Catalog could not be loaded'), findsOneWidget);
      expect(find.text('Retry'), findsOneWidget);
    });
  });

  group('Package details', () {
    testWidgets('unsupported platform disables install and explains why', (
      tester,
    ) async {
      await tester.pumpWidget(buildApp());
      await tester.pumpAndSettle();
      await goTo(tester, Icons.explore_outlined);

      final dockerCard = find.text('Docker Engine');
      await tester.ensureVisible(dockerCard);
      await tester.pumpAndSettle();
      await tester.tap(dockerCard);
      await tester.pumpAndSettle();

      // Clear warning, never a silent dead button (DESIGN.md §4/§8).
      expect(find.textContaining('does not list windows-x64'), findsOneWidget);

      // The primary action is below the fold; the list builds children
      // lazily, so drag until it exists.
      await tester.scrollUntilVisible(
        find.text('Install unavailable'),
        200,
        scrollable: find.byType(Scrollable).first,
      );
      await tester.pumpAndSettle();

      final installButton = filledButtonWithLabel('Install unavailable');
      expect(installButton, findsOneWidget);
      expect(tester.widget<FilledButton>(installButton).onPressed, isNull);
      expect(find.textContaining('Disabled: no artifact'), findsOneWidget);
    });

    testWidgets('trust status is accurate, never claims "trusted"', (
      tester,
    ) async {
      await tester.pumpWidget(buildApp());
      await tester.pumpAndSettle();
      await goTo(tester, Icons.explore_outlined);

      final dockerCard = find.text('Docker Engine');
      await tester.ensureVisible(dockerCard);
      await tester.pumpAndSettle();
      await tester.tap(dockerCard);
      await tester.pumpAndSettle();

      expect(
        find.textContaining('publisher signature not yet checked'),
        findsOneWidget,
      );
      expect(find.textContaining('trusted'), findsNothing);
    });

    testWidgets('latest policy resolves to the newest listed version', (
      tester,
    ) async {
      await tester.pumpWidget(buildApp());
      await tester.pumpAndSettle();
      await goTo(tester, Icons.explore_outlined);

      await tester.tap(find.text('Python'));
      await tester.pumpAndSettle();

      await tester.tap(find.text('Latest'));
      await tester.pumpAndSettle();

      // Python 3.13.1 is a beta channel release; "latest" includes it
      // while "stable" resolves to 3.12.7.
      expect(find.text('Resolves to: 3.13.1'), findsOneWidget);
      expect(find.text('Install 3.13.1'), findsOneWidget);
    });

    testWidgets('install shows a plan review and starts the resolved job', (
      tester,
    ) async {
      final recording = _RecordingJobsRepository();
      await tester.pumpWidget(
        buildApp(
          overrides: [jobsRepositoryProvider.overrideWithValue(recording)],
        ),
      );
      await tester.pumpAndSettle();
      await goTo(tester, Icons.explore_outlined);

      await tester.tap(find.text('Flutter'));
      await tester.pumpAndSettle();

      // Sanity: the details screen actually loaded before we look for the
      // primary action. The primary action sits below the fold; the list
      // builds children lazily, so drag until the button exists.
      expect(find.text('Version policy'), findsOneWidget);
      expect(find.text('Resolves to: 3.27.1'), findsOneWidget);

      await tester.scrollUntilVisible(
        find.text('Install 3.27.1'),
        200,
        scrollable: find.byType(Scrollable).first,
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Install 3.27.1'));
      await tester.pumpAndSettle();

      // Plan review shows the real facts before execution (DESIGN.md §8).
      expect(find.text('Review install — Flutter 3.27.1'), findsOneWidget);
      expect(find.textContaining('storage.example.invalid'), findsWidgets);
      expect(find.textContaining('BSD-3-Clause'), findsWidgets);
      expect(find.textContaining('simulated install job'), findsWidgets);

      await tester.tap(find.text('Start install'));
      await tester.pump();
      await tester.pump();

      expect(recording.started, ['devkit.framework.flutter|Flutter|3.27.1']);
      expect(
        find.textContaining('Install job started: test-job-1'),
        findsOneWidget,
      );

      await tester.pumpAndSettle();
      // The screen navigates to Downloads where the (empty) job list shows.
      expect(find.text('No downloads yet'), findsOneWidget);
    });
  });

  group('Profiles', () {
    testWidgets('optional packages are editable before applying', (
      tester,
    ) async {
      await tester.pumpWidget(buildApp());
      await tester.pumpAndSettle();
      await goTo(tester, Icons.people_outline);

      expect(find.text('Flutter Developer'), findsOneWidget);
      expect(find.text('Python Developer'), findsOneWidget);

      final flutterCard = find
          .ancestor(
            of: find.text('Flutter Developer'),
            matching: find.byType(Card),
          )
          .first;

      // Optional VS Code row: toggle it off the plan.
      final vscodeRow = find.descendant(
        of: flutterCard,
        matching: find.textContaining('devkit.ide.vscode'),
      );
      await tester.ensureVisible(vscodeRow);
      final tile = find.ancestor(
        of: vscodeRow,
        matching: find.byType(SwitchListTile),
      );
      expect(
        tester.widget<SwitchListTile>(tile).onChanged,
        isNotNull,
        reason: 'optional packages must be editable',
      );
      await tester.tap(
        find.descendant(of: tile, matching: find.byType(Switch)),
      );
      await tester.pumpAndSettle();

      expect(find.textContaining('2 of 3 packages'), findsOneWidget);
    });

    testWidgets('required packages cannot be deselected', (tester) async {
      await tester.pumpWidget(buildApp());
      await tester.pumpAndSettle();
      await goTo(tester, Icons.people_outline);

      final flutterCard = find
          .ancestor(
            of: find.text('Flutter Developer'),
            matching: find.byType(Card),
          )
          .first;
      final flutterRow = find.descendant(
        of: flutterCard,
        matching: find.textContaining('devkit.framework.flutter'),
      );
      await tester.ensureVisible(flutterRow);
      final tile = find.ancestor(
        of: flutterRow,
        matching: find.byType(SwitchListTile),
      );
      // Required packages: the switch is disabled (null callback).
      expect(tester.widget<SwitchListTile>(tile).onChanged, isNull);
    });

    testWidgets('apply shows the planned package list before executing', (
      tester,
    ) async {
      await tester.pumpWidget(buildApp());
      await tester.pumpAndSettle();
      await goTo(tester, Icons.people_outline);

      final flutterCard = find
          .ancestor(
            of: find.text('Flutter Developer'),
            matching: find.byType(Card),
          )
          .first;
      final apply = find.descendant(
        of: flutterCard,
        matching: find.text('Apply'),
      );
      await tester.ensureVisible(apply);
      await tester.tap(apply);
      await tester.pumpAndSettle();

      expect(find.text('Apply profile — Flutter Developer'), findsOneWidget);
      expect(find.textContaining('devkit.framework.flutter'), findsWidgets);
      expect(
        find.textContaining('simulates profile application'),
        findsWidgets,
      );

      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
    });
  });

  group('Installed', () {
    testWidgets('external software is locked, broken state is labeled', (
      tester,
    ) async {
      await tester.pumpWidget(buildApp());
      await tester.pumpAndSettle();
      await goTo(tester, Icons.inventory_2_outlined);

      expect(find.text('Visual Studio Code'), findsOneWidget);
      // Externally detected software: lock affordance, no delete.
      expect(find.byIcon(Icons.lock_outline), findsOneWidget);
      final vscodeRow = find
          .ancestor(
            of: find.text('Visual Studio Code'),
            matching: find.byType(Card),
          )
          .first;
      expect(
        find.descendant(
          of: vscodeRow,
          matching: find.byIcon(Icons.delete_outline),
        ),
        findsNothing,
      );

      expect(find.text('Broken'), findsOneWidget);
    });

    testWidgets('uninstall requires confirmation', (tester) async {
      await tester.pumpWidget(buildApp());
      await tester.pumpAndSettle();
      await goTo(tester, Icons.inventory_2_outlined);

      final gitRow = find
          .ancestor(of: find.text('Git'), matching: find.byType(Card))
          .first;
      final delete = find.descendant(
        of: gitRow,
        matching: find.byIcon(Icons.delete_outline),
      );

      await tester.tap(delete);
      await tester.pumpAndSettle();
      expect(find.text('Uninstall Git 2.51.0?'), findsOneWidget);

      // Cancel keeps the package.
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(find.text('Git'), findsOneWidget);

      // Confirming removes it.
      await tester.tap(delete);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Uninstall'));
      await tester.pumpAndSettle();
      expect(find.text('Git'), findsNothing);
    });
  });

  group('Downloads', () {
    testWidgets('history shows completed and failed jobs with error details', (
      tester,
    ) async {
      await tester.pumpWidget(buildApp());
      await tester.pumpAndSettle();
      await goTo(tester, Icons.download_outlined);

      expect(find.text('History'), findsOneWidget);
      expect(find.text('Git 2.50.1'), findsOneWidget);
      expect(find.text('Python 3.13.1'), findsOneWidget);
      expect(find.textContaining('INTEGRITY_CHECK_FAILED'), findsWidgets);

      // No active jobs: no cancel control, no animated progress bar.
      expect(find.text('Cancel'), findsNothing);
      expect(find.byType(LinearProgressIndicator), findsNothing);
      // Failed jobs offer retry; completed ones do not.
      expect(filledButtonWithLabel('Retry'), findsOneWidget);
      expect(filledButtonWithLabel('Cancel'), findsNothing);
    });

    testWidgets('structured log expands and shows the rollback chain', (
      tester,
    ) async {
      await tester.pumpWidget(buildApp());
      await tester.pumpAndSettle();
      await goTo(tester, Icons.download_outlined);

      final logTile = find.text('Structured log (5 events)');
      // The Python job card sits below the fold; drag the list until the
      // tile is built and visible.
      final listView = find.byType(ListView).first;
      var drags = 0;
      while (logTile.evaluate().isEmpty && drags < 6) {
        await tester.drag(listView, const Offset(0, -280));
        await tester.pumpAndSettle();
        drags++;
      }
      expect(logTile, findsOneWidget);
      await tester.tap(logTile);
      await tester.pumpAndSettle();

      // The failed job's log records the full rollback chain.
      expect(find.textContaining('ROLLED_BACK —'), findsOneWidget);
      expect(find.textContaining('ROLLING_BACK —'), findsOneWidget);
      expect(find.textContaining('Checksum mismatch'), findsOneWidget);
    });
  });

  group('ProgressStage', () {
    testWidgets('measurable progress shows a percentage', (tester) async {
      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(
            body: ProgressStage(stage: JobStage.downloading, progress: 0.5),
          ),
        ),
      );
      expect(find.text('Downloading'), findsOneWidget);
      expect(find.text('50%'), findsOneWidget);
      final indicator = tester.widget<LinearProgressIndicator>(
        find.byType(LinearProgressIndicator),
      );
      expect(indicator.value, 0.5);
    });

    testWidgets('indeterminate progress shows NO fake percentage', (
      tester,
    ) async {
      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(body: ProgressStage(stage: JobStage.verifying)),
        ),
      );
      expect(find.text('Verifying checksum'), findsOneWidget);
      final indicator = tester.widget<LinearProgressIndicator>(
        find.byType(LinearProgressIndicator),
      );
      expect(
        indicator.value,
        isNull,
        reason: 'the bar itself must be indeterminate',
      );
      expect(find.textContaining(RegExp(r'^\d+%$')), findsNothing);
    });
  });

  group('FakeJobsRepository (simulator semantics, no real timers)', () {
    test('cancellation rolls back at the next safe boundary', () {
      fakeAsync((async) {
        final jobs = FakeJobsRepository();
        String? id;
        jobs
            .startInstall(
              packageId: 'devkit.utility.git',
              packageLabel: 'Git',
              version: '2.51.0',
            )
            .then((value) => id = value);
        async.flushMicrotasks();
        expect(id, 'sim-job-1');

        // Active jobs sort before history.
        late List<JobInfo> initial;
        jobs.jobs().then((value) => initial = value);
        async.flushMicrotasks();
        expect(initial.first.id, id);

        jobs.cancelJob(id!);
        jobs.debugAdvanceOnce();

        late JobInfo job;
        jobs.jobs().then((value) => job = value.singleWhere((j) => j.id == id));
        async.flushMicrotasks();

        expect(job.stage, JobStage.rolledBack);
        expect(job.isActive, isFalse);
        expect(
          job.log.map((e) => e.stage),
          containsAllInOrder([
            JobStage.cancelled,
            JobStage.rollingBack,
            JobStage.rolledBack,
          ]),
        );
        // Terminal states reject further cancellation silently.
        jobs.cancelJob(id!);
        jobs.debugAdvanceOnce();
        expect(job.stage, JobStage.rolledBack);
      });
    });

    test('a simulated install walks the state machine to COMPLETED', () {
      fakeAsync((async) {
        final jobs = FakeJobsRepository();
        String? id;
        jobs
            .startInstall(
              packageId: 'devkit.lang.python',
              packageLabel: 'Python',
              version: '3.13.1',
            )
            .then((value) => id = value);
        async.flushMicrotasks();

        JobInfo snapshot() {
          late JobInfo out;
          jobs.jobs().then(
            (value) => out = value.singleWhere((j) => j.id == id),
          );
          async.flushMicrotasks();
          return out;
        }

        var job = snapshot();
        expect(job.stage, JobStage.queued);

        // 5 ticks: RESOLVING, PREPARING, then three DOWNLOADING chunks.
        for (var i = 0; i < 5; i++) {
          jobs.debugAdvanceOnce();
        }
        job = snapshot();
        expect(job.stage, JobStage.downloading);
        expect(job.progress, closeTo(0.75, 0.001));

        // Walk to the terminal state.
        var ticks = 0;
        while (!job.stage.isTerminal && ticks < 20) {
          jobs.debugAdvanceOnce();
          job = snapshot();
          ticks++;
        }
        expect(job.stage, JobStage.completed);

        // DOWNLOADING progress is measurable; VERIFYING is indeterminate
        // and must carry no progress value (no fake percentages).
        expect(
          job.log
              .where((e) => e.stage == JobStage.downloading)
              .every((e) => e.progress != null),
          isTrue,
        );
        final verifying = job.log
            .where((e) => e.stage == JobStage.verifying)
            .toList();
        expect(verifying, isNotEmpty);
        expect(verifying.every((e) => e.progress == null), isTrue);

        // Terminal jobs reject further cancellation.
        jobs.cancelJob(id!);
        jobs.debugAdvanceOnce();
        expect(snapshot().stage, JobStage.completed);
      });
    });
  });
}

/// Catalog stub that stays in the loading state until released — drives
/// the loading-spinner assertion without pending timers.
class _DelayedCatalog implements CatalogRepository {
  final Completer<List<CatalogPackage>> _completer = Completer();
  final FixtureCatalogRepository _inner = FixtureCatalogRepository();

  /// Completes the pending load with the real fixture catalog.
  Future<void> release() async => _completer.complete(_inner.packages());

  @override
  Future<List<CatalogPackage>> packages() => _completer.future;

  @override
  Future<CatalogPackage?> package(String id) async => null;

  @override
  Future<List<ProfileInfo>> profiles() async => const [];
}

/// Catalog stub that always fails — drives the error-state UI.
class _ExplodingCatalogRepository implements CatalogRepository {
  @override
  Future<List<CatalogPackage>> packages() async {
    throw StateError('catalog offline');
  }

  @override
  Future<CatalogPackage?> package(String id) async => null;

  @override
  Future<List<ProfileInfo>> profiles() async => const [];
}

/// Jobs stub that records starts without spawning timers — used so the
/// install-plan-review widget test never owns a live simulated job.
class _RecordingJobsRepository implements JobsRepository {
  final List<String> started = [];

  @override
  Future<List<JobInfo>> jobs() async => const <JobInfo>[];

  @override
  Stream<List<JobInfo>> watchJobs() =>
      Stream<List<JobInfo>>.value(const <JobInfo>[]);

  @override
  Future<String> startInstall({
    required String packageId,
    required String packageLabel,
    required String version,
  }) async {
    started.add('$packageId|$packageLabel|$version');
    return 'test-job-1';
  }

  @override
  Future<void> cancelJob(String jobId) async {}
}
