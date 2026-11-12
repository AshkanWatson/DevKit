import 'package:devkit/app/app.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

void main() {
  // The ProviderScope must wrap the app root: DevKitApp is a Riverpod
  // ConsumerWidget and reads the router provider at build time. A missing
  // scope throws ProviderScopeNotFound, which in release builds appears as
  // a blank white window (regression test: test/widget_test.dart).
  runApp(const DevKitRoot());
}

/// The real composition root used by `main` and by tests, so tests verify
/// the exact widget tree that ships.
class DevKitRoot extends StatelessWidget {
  const DevKitRoot({super.key});

  @override
  Widget build(BuildContext context) {
    return const ProviderScope(child: DevKitApp());
  }
}
