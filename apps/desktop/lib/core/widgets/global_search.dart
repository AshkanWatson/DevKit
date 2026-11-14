import 'package:material_ui/material_ui.dart';

/// Global search field (DESIGN.md §7). Keyboard-accessible with visible
/// focus; callers own the value so query state survives navigation.
class GlobalSearch extends StatefulWidget {
  const GlobalSearch({super.key, required this.value, required this.onChanged});

  final String value;
  final ValueChanged<String> onChanged;

  @override
  State<GlobalSearch> createState() => _GlobalSearchState();
}

class _GlobalSearchState extends State<GlobalSearch> {
  // The state owns its controller so provider-driven rebuilds never reset
  // the caret or IME composition mid-typing; [GlobalSearch.value] stays the
  // source of truth and is applied on external changes (e.g. the clear
  // button on another widget instance).
  late final TextEditingController _controller = TextEditingController(
    text: widget.value,
  );

  @override
  void didUpdateWidget(covariant GlobalSearch oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.value != _controller.text) {
      _controller.value = TextEditingValue(
        text: widget.value,
        selection: TextSelection.collapsed(offset: widget.value.length),
      );
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return TextField(
      controller: _controller,
      decoration: InputDecoration(
        prefixIcon: const Icon(Icons.search),
        hintText: 'Search packages, publishers, categories…',
        suffixIcon: widget.value.isEmpty
            ? null
            : IconButton(
                icon: const Icon(Icons.clear),
                tooltip: 'Clear search',
                onPressed: () => widget.onChanged(''),
              ),
        border: const OutlineInputBorder(
          borderRadius: BorderRadius.all(Radius.circular(12)),
        ),
        isDense: true,
      ),
      style: theme.textTheme.bodyMedium,
      onChanged: widget.onChanged,
    );
  }
}
