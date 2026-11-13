import 'package:devkit/app/app.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
// `Override` lives in riverpod's "misc" public surface in Riverpod 3.x;
// flutter_riverpod does not re-export it.
import 'package:riverpod/misc.dart';

void main() {
  // The ProviderScope must wrap the app root: DevKitApp is a Riverpod
  // ConsumerWidget and reads the router provider at build time. A missing
  // scope throws ProviderScopeNotFound, which in release builds appears as
  // a blank white window (regression test: test/widget_test.dart).
  runApp(const DevKitRoot());
}

/// The real composition root used by `main` and by tests, so tests verify
/// the exact widget tree that ships.
///
/// [overrides] lets tests inject fakes for providers that touch real FFI
/// (the bridge dynamic library is not loadable in the test environment).
class DevKitRoot extends StatelessWidget {
  const DevKitRoot({super.key, this.overrides = const []});

  final List<Override> overrides;

  @override
  Widget build(BuildContext context) {
    return ProviderScope(overrides: overrides, child: const DevKitApp());
  }
}
