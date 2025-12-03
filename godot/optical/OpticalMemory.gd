extends Control

@export var bridge_path: NodePath = NodePath("/root/DaemonBridge")

var _bridge: Node

func _ready() -> void:
	if bridge_path != NodePath(""):
		_bridge = get_node_or_null(bridge_path)
	if _bridge:
		_bridge.render_optical_memory_requested.connect(_on_render_request)

func _on_render_request(chat_history: Array, memory_nodes: Array) -> void:
	var memory_image := _render_memory_map(memory_nodes)
	var chat_image := _render_chat_transcript(chat_history)
	var status_image := _render_status_panel(memory_nodes.size())

	var memory_bytes := memory_image.save_png_to_buffer()
	var chat_bytes := chat_image.save_png_to_buffer()
	var status_bytes := status_image.save_png_to_buffer()
	if _bridge:
		_bridge.send_rendered_images(memory_bytes, chat_bytes, status_bytes)

func _render_memory_map(memory_nodes: Array) -> Image:
	var img := Image.create(512, 512, false, Image.FORMAT_RGBA8)
	img.fill(Color.DARK_SLATE_GRAY)
	for i in range(memory_nodes.size()):
		var angle: float = float(i) / max(1.0, float(memory_nodes.size()))
		var px := int(256 + cos(angle * TAU) * 200.0)
		var py := int(256 + sin(angle * TAU) * 200.0)
		var color := Color.from_hsv(angle, 0.7, 0.9)
		img.set_pixel(px % 512, py % 512, color)
	return img

func _render_chat_transcript(chat_history: Array) -> Image:
	var img := Image.create(512, 512, false, Image.FORMAT_RGBA8)
	img.fill(Color.DIM_GRAY)
	var y := 16
	for message in chat_history:
		var text := "%s: %s" % [message.get("sender", "unknown"), message.get("content", "")]
		_draw_text(img, text, Vector2i(12, y), Color.WHITE)
		y += 20
		if y > 480:
			break
	return img

func _draw_text(img: Image, text: String, origin: Vector2i, color: Color) -> void:
	var chars := text.to_utf8_buffer()
	for i in chars.size():
		var x := origin.x + i * 6
		for y_off in range(12):
			var pos := Vector2i(x, origin.y + y_off)
			if pos.x >= 0 and pos.x < img.get_width() and pos.y >= 0 and pos.y < img.get_height():
				img.set_pixel(pos.x, pos.y, color)

func _render_status_panel(count: int) -> Image:
	var img := Image.create(512, 512, false, Image.FORMAT_RGBA8)
	img.fill(Color(0.1, 0.1, 0.12, 1.0))
	for i in range(count):
		var height := int(400.0 * float(i + 1) / max(count, 1))
		for x in range(40 * i + 20, 40 * i + 40):
			for y in range(480 - height, 480):
				if x < 512 and y < 512:
					img.set_pixel(x, y, Color(0.3 + 0.1 * i, 0.8, 0.3, 1.0))
	return img

