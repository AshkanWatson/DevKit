/// P3-05 tests: the FFI repository mapping layer and the provider
/// defaults. These run without real FFI — the generated bridge DTO
/// classes are plain Dart objects, so the mappers are exercised by
/// constructing them directly. Widget tests never touch real FFI: the
/// provider defaults fall back to the fakes when FLUTTER_TEST is set.
library;

import 'dart:convert';

import 'package:devkit/core/bridge/api/catalog.dart' as bridge_catalog;
import 'package:devkit/core/bridge/api/installed.dart' as bridge_installed;
import 'package:devkit/core/bridge/api/jobs.dart' as bridge_jobs;
import 'package:devkit/core/data/ffi_repositories.dart';
import 'package:devkit/core/data/fake_repositories.dart';
import 'package:devkit/core/domain/domain_models.dart';
import 'package:devkit/core/domain/repository_contracts.dart';
import 'package:devkit/core/providers/repository_providers.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

bridge_catalog.CatalogPackageDto _catalogDto() {
  return bridge_catalog.CatalogPackageDto(
    id: 'devkit.lang.python',
    name: 'Python',
    description: 'The Python programming language.',
    publisher: 'Python Software Foundation',
    homepage: 'https://www.python.org',
    category: 'language',
    platforms: const ['windows-x64', 'linux-x64'],
    defaultPolicy: 'stable',
    license: 'PSF-2.0',
    maintainer: 'catalog maintainers',
    updatedAt: '2026-01-01T00:00:00Z',
    versions: [
      // Deliberately NOT newest-first: the mapper must normalize.
      bridge_catalog.CatalogVersionDto(
        version: '3.12.7',
        channel: 'stable',
        releaseDate: '2025-12-01',
        releaseNotesUrl: null,
        dependencies: [
          bridge_catalog.CatalogDependencyDto(
            packageId: 'devkit.build.tool.pip',
            required_: false,
            policy: 'latest',
            version: null,
            channel: null,
          ),
        ],
        artifacts: [
          bridge_catalog.CatalogArtifactDto(
            os: 'windows',
            architecture: 'x86_64',
            installer: 'archive',
            url: 'https://www.python.org/ftp/python/3.12.7/python-3.12.7-amd64.zip?sig=secret',
            sha256: 'A' * 64,
            sizeBytes: BigInt.from(12345678),
            archiveRoot: '',
            executableRelativePath: 'python.exe',
          ),
        ],
      ),
      bridge_catalog.CatalogVersionDto(
        version: '3.13.1',
        channel: 'stable',
        releaseDate: '2026-01-15',
        releaseNotesUrl: null,
        dependencies: const [],
        artifacts: const [],
      ),
    ],
  );
}

void main() {
  group('catalog mapping (offline cache → UI read model)', () {
    test('maps fields and normalizes versions newest-first', () {
      final package = catalogPackageFromDto(_catalogDto());
      expect(package.id, 'devkit.lang.python');
      expect(package.category, PackageCategory.language);
      expect(package.defaultPolicy, VersionPolicyName.stable);
      expect(package.platforms, hasLength(2));
      expect(package.newestStable?.version, '3.13.1');
      expect(package.license, 'PSF-2.0');
      final artifact = package.versions
          .firstWhere((v) => v.version == '3.12.7')
          .artifacts
          .single;
      expect(artifact.installer, InstallerKind.archive);
      expect(artifact.sizeBytes, 12345678);
      expect(artifact.hasChecksum, isTrue);
      expect(artifact.sourceDomain, 'www.python.org');
      expect(artifact.sourceDomain.contains('?sig=secret'), isFalse);
      expect(artifact.sizeLabel, '12.3 MB');
    });

    test('maps dependencies and profiles', () {
      final package = catalogPackageFromDto(_catalogDto());
      final dependency = package.versions
          .firstWhere((v) => v.version == '3.12.7')
          .dependencies
          .single;
      expect(dependency.packageId, 'devkit.build.tool.pip');
      expect(dependency.required, isFalse);
      expect(dependency.policy, VersionPolicyName.latest);

      final profile = profileInfoFromDto(
        bridge_catalog.CatalogProfileDto(
          id: 'devkit.profile.web',
          name: 'Web development',
          description: 'Front-end starter set.',
          maintainer: 'catalog maintainers',
          platforms: const ['windows-x64'],
          packages: [
            bridge_catalog.CatalogProfilePackageDto(
              packageId: 'devkit.lang.python',
              required_: true,
              policy: 'stable',
              version: null,
              channel: null,
              reason: 'Backend scripting.',
            ),
          ],
        ),
      );
      expect(profile.packages.single.required, isTrue);
      expect(profile.packages.single.policy, VersionPolicyName.stable);
      expect(profile.packages.single.reason, 'Backend scripting.');
    });
  });

  group('installed mapping (SQLite registry → UI read model)', () {
    test('maps the managed row and status wire strings', () {
      final installed = installedPackageFromDto(
        bridge_installed.InstalledPackageDto(
          packageId: 'devkit.lang.python',
          name: 'Python',
          version: '3.12.7',
          status: 'installed',
          isActive: true,
          locationLabel: r'packages\devkit.lang.python\3.12.7',
          installedAt: '2026-01-01T00:00:00Z',
        ),
      );
      expect(installed.status, InstalledStatus.installed);
      expect(installed.isActive, isTrue);
      expect(installed.ownership, InstallOwnership.devkitManaged);
      expect(installed.locationLabel, r'packages\devkit.lang.python\3.12.7');

      final broken = installedPackageFromDto(
        bridge_installed.InstalledPackageDto(
          packageId: 'devkit.lang.rust',
          name: 'Rust (toolchain)',
          version: '1.97.1',
          status: 'broken',
          isActive: false,
          locationLabel: r'packages\devkit.lang.rust\1.97.1',
          installedAt: '2026-01-01T00:00:00Z',
        ),
      );
      expect(broken.status, InstalledStatus.broken);
    });

    test('phrases the PATH change exactly like the Phase 1 consent copy', () {
      expect(
        pathChangeLabel(
          bridge_installed.PathChangeDto(
            kind: bridge_installed.PathChangeKind.append,
          ),
        ),
        'Add the DevKit shims folder to your user PATH (user scope '
        'only; no administrator rights required).',
      );
      expect(
        pathChangeLabel(
          bridge_installed.PathChangeDto(
            kind: bridge_installed.PathChangeKind.alreadyPresent,
          ),
        ),
        'The DevKit shims folder is already on your user PATH — '
        'no PATH change is needed.',
      );
      expect(
        pathChangeLabel(
          bridge_installed.PathChangeDto(
            kind: bridge_installed.PathChangeKind.unavailable,
            reason: 'user PATH integration is Windows-only in this release',
          ),
        ),
        'user PATH integration is Windows-only in this release',
      );
    });

    test('maps the activation plan (consent-dialog data)', () {
      final plan = activationPlanFromDto(
        bridge_installed.ActivationPlanDto(
          packageId: 'devkit.lang.python',
          version: '3.12.7',
          previousActiveVersion: '3.11.9',
          shims: [
            bridge_installed.ShimPlanDto(
              shimRelativePath: r'shims\python.cmd',
              targetRelativePath:
                  r'packages\devkit.lang.python\3.12.7\python.exe',
            ),
          ],
          shimsDirRelative: 'shims',
          pathChange: bridge_installed.PathChangeDto(
            kind: bridge_installed.PathChangeKind.append,
          ),
        ),
      );
      expect(plan.shimFiles, [r'shims\python.cmd']);
      expect(plan.reversible, isTrue);
      expect(plan.previousActiveVersion, '3.11.9');
      expect(plan.pathChange, startsWith('Add the DevKit shims folder'));
    });
  });

  group('job mapping (persisted events → UI read model)', () {
    test('maps stage wire forms, log tail, and display-name fallback', () {
      final job = jobInfoFromDto(
        bridge_jobs.JobDto(
          id: 'job-1',
          packageId: 'devkit.lang.python',
          packageName: 'Python',
          version: '3.12.7',
          state: 'DOWNLOADING',
          errorCode: null,
          createdAt: '2026-01-01T00:00:00Z',
          updatedAt: '2026-01-01T00:00:01Z',
          finishedAt: null,
          progress: 0.5,
          events: [
            bridge_jobs.JobEventDto(
              jobId: 'job-1',
              seq: 1,
              timestamp: '2026-01-01T00:00:00Z',
              stage: 'RESOLVING',
              progress: null,
              message: 'Resolving devkit.lang.python 3.12.7 from the catalog',
              errorCode: null,
            ),
            bridge_jobs.JobEventDto(
              jobId: 'job-1',
              seq: 2,
              timestamp: '2026-01-01T00:00:01Z',
              stage: 'DOWNLOADING',
              progress: 0.5,
              message: 'Downloading',
              errorCode: null,
            ),
          ],
        ),
      );
      expect(job.packageLabel, 'Python');
      expect(job.stage, JobStage.downloading);
      expect(job.isActive, isTrue);
      expect(job.canBeCancelled, isTrue);
      expect(job.progress, 0.5, reason: 'last event of the current stage');
      expect(job.log.last.stage, JobStage.downloading);
    });

    test('unknown package falls back to the stable ID for the label', () {
      final job = jobInfoFromDto(
        bridge_jobs.JobDto(
          id: 'job-2',
          packageId: 'devkit.bridge.missing',
          packageName: null,
          version: '1.0.0',
          state: 'ROLLED_BACK',
          errorCode: 'PACKAGE_NOT_FOUND',
          createdAt: '2026-01-01T00:00:00Z',
          updatedAt: '2026-01-01T00:00:02Z',
          finishedAt: '2026-01-01T00:00:02Z',
          progress: null,
          events: const [],
        ),
      );
      expect(job.packageLabel, 'devkit.bridge.missing');
      expect(job.stage, JobStage.rolledBack);
      expect(job.isActive, isFalse);
      expect(job.canBeRetried, isTrue);
      expect(job.errorCode, 'PACKAGE_NOT_FOUND');
    });
  });

  group('provider defaults', () {
    // FLUTTER_TEST is set by the Flutter test runner: the defaults must
    // be the fakes so widget tests never touch real FFI.
    test('fall back to the fakes under flutter test', () {
      final container = ProviderContainer();
      addTearDown(container.dispose);
      expect(
        container.read(catalogRepositoryProvider),
        isA<FixtureCatalogRepository>(),
      );
      expect(
        container.read(installedRepositoryProvider),
        isA<FakeInstalledRepository>(),
      );
      expect(container.read(jobsRepositoryProvider), isA<FakeJobsRepository>());
    });

    test('fakes and FFI implementations share the domain contracts', () {
      // The P1-08 final proof: the real repositories implement the very
      // interfaces the Phase 1 screens were built against.
      expect(FfiCatalogRepository(), isA<CatalogRepository>());
      expect(FfiInstalledRepository(), isA<InstalledRepository>());
      expect(FfiJobsRepository(), isA<JobsRepository>());
    });
  });

  group('profile resolution (P4-02 reviewable plan)', () {
    test('fake resolves a seeded profile into concrete versions', () async {
      final repo = FixtureCatalogRepository();
      final plan = await repo.resolveProfile('devkit.profile.python-developer');
      expect(plan.profileId, 'devkit.profile.python-developer');
      // Python is pinned exact to 3.12.7 in the fixture.
      final python = plan.packages.firstWhere(
        (p) => p.packageId == 'devkit.lang.python',
      );
      expect(python.version, '3.12.7');
      expect(python.policy, 'exact');
      expect(python.required, isTrue);
    });

    test('ProfileSkipReason maps wire forms to domain values', () {
      expect(
        ProfileSkipReason.fromWire('packageUnavailable'),
        ProfileSkipReason.packageUnavailable,
      );
      expect(
        ProfileSkipReason.fromWire('noMatchingVersion'),
        ProfileSkipReason.noMatchingVersion,
      );
      expect(
        ProfileSkipReason.fromWire('deselected'),
        ProfileSkipReason.deselected,
      );
    });

    test(
      'personal profiles save/delete and never shadow a catalog profile',
      () async {
        final repo = FixtureCatalogRepository();
        expect(await repo.personalProfiles(), isEmpty);

        final personal = ProfileInfo(
          id: 'devkit.profile.personal.test',
          name: 'Mine',
          description: 'my profile',
          maintainer: 'user',
          platforms: const [],
          packages: const [],
        );
        final saved = await repo.savePersonalProfile(
          jsonEncode(<String, dynamic>{
            'schema_version': 1,
            'id': personal.id,
            'name': personal.name,
            'description': personal.description,
            'packages': const [],
          }),
        );
        expect(saved.id, personal.id);
        expect(await repo.personalProfiles(), hasLength(1));

        // A catalog profile id cannot be overwritten as personal.
        expect(
          () => repo.savePersonalProfile(
            jsonEncode(<String, dynamic>{
              'schema_version': 1,
              'id': 'devkit.profile.flutter-developer',
              'name': 'Hijack',
              'description': '',
              'packages': const [],
            }),
          ),
          throwsStateError,
        );

        expect(await repo.deletePersonalProfile(personal.id), isTrue);
        expect(await repo.personalProfiles(), isEmpty);
        expect(await repo.deletePersonalProfile(personal.id), isFalse);
      },
    );
  });
}
