extends Control
## Drag handle for moving the borderless window
##
## Click and drag anywhere on this control to move the window.

var _dragging: bool = false
var _drag_start_mouse: Vector2i = Vector2i.ZERO
var _drag_start_window: Vector2i = Vector2i.ZERO


func _ready() -> void:
	# Enable mouse input
	mouse_filter = Control.MOUSE_FILTER_STOP
	mouse_default_cursor_shape = Control.CURSOR_MOVE


func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		var mb = event as InputEventMouseButton
		if mb.button_index == MOUSE_BUTTON_LEFT:
			if mb.pressed:
				_dragging = true
				# Store the starting positions using SCREEN coordinates
				_drag_start_mouse = DisplayServer.mouse_get_position()
				_drag_start_window = get_window().position
			else:
				_dragging = false


func _process(_delta: float) -> void:
	if _dragging:
		# Get current mouse position in screen coordinates
		var current_mouse = DisplayServer.mouse_get_position()
		# Calculate how far we've moved from the start
		var delta = current_mouse - _drag_start_mouse
		# Apply that delta to the original window position
		get_window().position = _drag_start_window + delta
