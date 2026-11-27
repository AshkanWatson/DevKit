// Cross-platform tolerant golden comparator (P5-02).
//
// Golden PNGs are generated on one platform and committed; CI runs the same
// test on ubuntu-latest, macos-latest and windows-latest, where font
// rasterization differs slightly. The default byte-for-byte comparator would
// fail on those differences, so this comparator tolerates a small percentage
// of differing pixels (the documented flutter_test tolerance pattern) while
// still catching real layout/content regressions.

import 'dart:typed_data';

import 'package:flutter/foundation.dart' show FlutterError;
import 'package:flutter_test/flutter_test.dart';

/// Fraction of pixels allowed to differ (font antialiasing across OSes).
const double _precisionTolerance = 0.015;

Future<void> testExecutable(Future<void> Function() testMain) async {
  goldenFileComparator = _TolerantGoldenFileComparator(
    Uri.parse('test/golden_test.dart'),
  );
  await testMain();
}

class _TolerantGoldenFileComparator extends LocalFileComparator {
  _TolerantGoldenFileComparator(super.testFile);

  @override
  Future<bool> compare(Uint8List imageBytes, Uri golden) async {
    final result = await GoldenFileComparator.compareLists(
      imageBytes,
      await getGoldenBytes(golden),
    );
    final passed = result.passed || result.diffPercent <= _precisionTolerance;
    if (passed) {
      result.dispose();
      return true;
    }
    final error = await generateFailureOutput(result, golden, basedir);
    result.dispose();
    throw FlutterError(error);
  }
}
