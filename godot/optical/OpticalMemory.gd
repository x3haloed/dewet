extends Control
## Renders optical memory visualizations for the VLM composite image
##
## Uses SubViewports to render proper text that the VLM can read.
## Also renders ARIAOS - the companion's self-managed display.

@export var bridge_path: NodePath = NodePath("/root/DaemonBridge")

const PANEL_SIZE := Vector2i(512, 512)
const ARIAOS_SIZE := Vector2i(1024, 768)

var _bridge: Node

# Viewports for rendering each panel
var _memory_viewport: SubViewport
var _chat_viewport: SubViewport
var _status_viewport: SubViewport
var _ariaos_viewport: SubViewport

# UI elements in each viewport
var _memory_container: VBoxContainer
var _chat_container: VBoxContainer
var _status_container: VBoxContainer
var _ariaos_container: Control



func _ready() -> void:
	_setup_viewports()
	_setup_ariaos_viewport()
	
	if bridge_path != NodePath(""):
		_bridge = get_node_or_null(bridge_path)
	if _bridge:
		_bridge.render_optical_memory_requested.connect(_on_render_request)
		if _bridge.has_signal("render_ariaos_requested"):
			_bridge.render_ariaos_requested.connect(_on_ariaos_render_request)


func _setup_viewports() -> void:
	# Memory Map Viewport
	_memory_viewport = SubViewport.new()
	_memory_viewport.size = PANEL_SIZE
	_memory_viewport.transparent_bg = false
	_memory_viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
	add_child(_memory_viewport)
	
	var memory_bg := ColorRect.new()
	memory_bg.color = Color(0.12, 0.14, 0.18, 1.0)
	memory_bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	_memory_viewport.add_child(memory_bg)
	
	var memory_title := Label.new()
	memory_title.text = "MEMORY MAP"
	memory_title.position = Vector2(16, 12)
	memory_title.add_theme_font_size_override("font_size", 18)
	memory_title.add_theme_color_override("font_color", Color(0.6, 0.7, 0.9, 1.0))
	_memory_viewport.add_child(memory_title)
	
	_memory_container = VBoxContainer.new()
	_memory_container.position = Vector2(16, 48)
	_memory_container.size = Vector2(480, 440)
	_memory_viewport.add_child(_memory_container)
	
	# Chat Transcript Viewport
	_chat_viewport = SubViewport.new()
	_chat_viewport.size = PANEL_SIZE
	_chat_viewport.transparent_bg = false
	_chat_viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
	add_child(_chat_viewport)
	
	var chat_bg := ColorRect.new()
	chat_bg.color = Color(0.08, 0.09, 0.11, 1.0)
	chat_bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	_chat_viewport.add_child(chat_bg)
	
	var chat_title := Label.new()
	chat_title.text = "RECENT CHAT"
	chat_title.position = Vector2(16, 12)
	chat_title.add_theme_font_size_override("font_size", 18)
	chat_title.add_theme_color_override("font_color", Color(0.9, 0.7, 0.6, 1.0))
	_chat_viewport.add_child(chat_title)
	
	_chat_container = VBoxContainer.new()
	_chat_container.position = Vector2(16, 48)
	_chat_container.size = Vector2(480, 440)
	_chat_viewport.add_child(_chat_container)
	
	# Status Panel Viewport
	_status_viewport = SubViewport.new()
	_status_viewport.size = PANEL_SIZE
	_status_viewport.transparent_bg = false
	_status_viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
	add_child(_status_viewport)
	
	var status_bg := ColorRect.new()
	status_bg.color = Color(0.10, 0.12, 0.14, 1.0)
	status_bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	_status_viewport.add_child(status_bg)
	
	var status_title := Label.new()
	status_title.text = "COMPANIONS"
	status_title.position = Vector2(16, 12)
	status_title.add_theme_font_size_override("font_size", 18)
	status_title.add_theme_color_override("font_color", Color(0.6, 0.9, 0.7, 1.0))
	_status_viewport.add_child(status_title)
	
	_status_container = VBoxContainer.new()
	_status_container.position = Vector2(16, 48)
	_status_container.size = Vector2(480, 440)
	_status_viewport.add_child(_status_container)


func _setup_ariaos_viewport() -> void:
	_ariaos_viewport = SubViewport.new()
	_ariaos_viewport.size = ARIAOS_SIZE
	_ariaos_viewport.transparent_bg = false
	_ariaos_viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
	add_child(_ariaos_viewport)
	
	# ARIAOS background - deep space blue with subtle gradient feel
	var bg := ColorRect.new()
	bg.color = Color(0.06, 0.08, 0.12, 1.0)
	bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	_ariaos_viewport.add_child(bg)
	
	# Top bar / header
	var header := ColorRect.new()
	header.color = Color(0.10, 0.12, 0.18, 1.0)
	header.position = Vector2(0, 0)
	header.size = Vector2(ARIAOS_SIZE.x, 40)
	_ariaos_viewport.add_child(header)
	
	var header_title := Label.new()
	header_title.text = "ARIAOS"
	header_title.position = Vector2(16, 8)
	header_title.add_theme_font_size_override("font_size", 20)
	header_title.add_theme_color_override("font_color", Color(0.95, 0.6, 0.75, 1.0))
	_ariaos_viewport.add_child(header_title)
	
	var header_subtitle := Label.new()
	header_subtitle.text = "Agent-Rendered Interactive Ambient OS"
	header_subtitle.position = Vector2(110, 12)
	header_subtitle.add_theme_font_size_override("font_size", 14)
	header_subtitle.add_theme_color_override("font_color", Color(0.5, 0.55, 0.65, 1.0))
	_ariaos_viewport.add_child(header_subtitle)
	
	# Status indicator in header
	var status_dot := ColorRect.new()
	status_dot.color = Color(0.3, 0.9, 0.5, 1.0)
	status_dot.position = Vector2(ARIAOS_SIZE.x - 80, 15)
	status_dot.size = Vector2(10, 10)
	_ariaos_viewport.add_child(status_dot)
	
	var status_label := Label.new()
	status_label.text = "ONLINE"
	status_label.position = Vector2(ARIAOS_SIZE.x - 65, 11)
	status_label.add_theme_font_size_override("font_size", 12)
	status_label.add_theme_color_override("font_color", Color(0.3, 0.9, 0.5, 1.0))
	_ariaos_viewport.add_child(status_label)
	
	# Main content area
	_ariaos_container = Control.new()
	_ariaos_container.position = Vector2(0, 40)
	_ariaos_container.size = Vector2(ARIAOS_SIZE.x, ARIAOS_SIZE.y - 40)
	_ariaos_viewport.add_child(_ariaos_container)
	
	# Populate with demo widgets
	_populate_ariaos_demo()


func _populate_ariaos_demo() -> void:
	# Clear existing
	for child in _ariaos_container.get_children():
		child.queue_free()
	
	# === Left column: Focus & Notes ===
	var left_panel := _create_ariaos_window("Current Focus", Vector2(16, 16), Vector2(480, 200))
	_ariaos_container.add_child(left_panel)
	
	var focus_content := VBoxContainer.new()
	focus_content.position = Vector2(12, 40)
	focus_content.size = Vector2(456, 150)
	left_panel.add_child(focus_content)
	
	var focus_text := Label.new()
	focus_text.text = "User is working on code in their IDE"
	focus_text.add_theme_font_size_override("font_size", 16)
	focus_text.add_theme_color_override("font_color", Color(0.85, 0.87, 0.92))
	focus_text.autowrap_mode = TextServer.AUTOWRAP_WORD
	focus_content.add_child(focus_text)
	
	var focus_bar_label := Label.new()
	focus_bar_label.text = "Engagement Level"
	focus_bar_label.add_theme_font_size_override("font_size", 12)
	focus_bar_label.add_theme_color_override("font_color", Color(0.5, 0.55, 0.6))
	focus_content.add_child(focus_bar_label)
	
	var focus_bar := _create_progress_bar(0.73, Color(0.4, 0.75, 0.95))
	focus_content.add_child(focus_bar)
	
	# Notes panel
	var notes_panel := _create_ariaos_window("Aria's Notes", Vector2(16, 232), Vector2(480, 280))
	_ariaos_container.add_child(notes_panel)
	
	var notes_content := VBoxContainer.new()
	notes_content.position = Vector2(12, 40)
	notes_content.size = Vector2(456, 230)
	notes_panel.add_child(notes_content)
	
	var notes := [
		{"icon": "📝", "text": "User seems focused - avoid interruptions"},
		{"icon": "⏰", "text": "Remind about break at 3:00 PM"},
		{"icon": "💡", "text": "They asked about ARIAOS earlier"},
		{"icon": "🎯", "text": "Current project: dewet daemon"},
	]
	
	for note in notes:
		var note_row := _create_note_row(note.icon, note.text)
		notes_content.add_child(note_row)
	
	# === Right column: System & Activity ===
	var system_panel := _create_ariaos_window("System Status", Vector2(512, 16), Vector2(496, 160))
	_ariaos_container.add_child(system_panel)
	
	var system_content := VBoxContainer.new()
	system_content.position = Vector2(12, 40)
	system_content.size = Vector2(472, 110)
	system_panel.add_child(system_content)
	
	var metrics := [
		{"label": "Memory Nodes", "value": "12 active"},
		{"label": "Chat Context", "value": "8 hot, 4 warm, 2 cold"},
		{"label": "Last Response", "value": "2 min ago"},
	]
	
	for metric in metrics:
		var metric_row := _create_metric_row(metric.label, metric.value)
		system_content.add_child(metric_row)
	
	# Activity log panel
	var activity_panel := _create_ariaos_window("Recent Activity", Vector2(512, 192), Vector2(496, 320))
	_ariaos_container.add_child(activity_panel)
	
	var activity_content := VBoxContainer.new()
	activity_content.position = Vector2(12, 40)
	activity_content.size = Vector2(472, 270)
	activity_panel.add_child(activity_content)
	
	var activities := [
		{"time": "10:42", "event": "Observed user switching to terminal"},
		{"time": "10:40", "event": "Decided not to interrupt (focus high)"},
		{"time": "10:38", "event": "Responded to user question"},
		{"time": "10:35", "event": "Detected project context change"},
		{"time": "10:30", "event": "Session started"},
	]
	
	for activity in activities:
		var activity_row := _create_activity_row(activity.time, activity.event)
		activity_content.add_child(activity_row)


func _create_ariaos_window(title: String, pos: Vector2, window_size: Vector2) -> Control:
	var window := Control.new()
	window.position = pos
	window.size = window_size
	
	# Window background
	var bg := ColorRect.new()
	bg.color = Color(0.10, 0.12, 0.16, 1.0)
	bg.size = window_size
	window.add_child(bg)
	
	# Window border (subtle)
	var border := ColorRect.new()
	border.color = Color(0.2, 0.24, 0.32, 1.0)
	border.size = Vector2(window_size.x, 2)
	window.add_child(border)
	
	# Title bar
	var title_bar := ColorRect.new()
	title_bar.color = Color(0.14, 0.16, 0.22, 1.0)
	title_bar.position = Vector2(0, 2)
	title_bar.size = Vector2(window_size.x, 32)
	window.add_child(title_bar)
	
	# Window controls (decorative)
	var close_btn := ColorRect.new()
	close_btn.color = Color(0.9, 0.4, 0.4, 0.8)
	close_btn.position = Vector2(10, 12)
	close_btn.size = Vector2(10, 10)
	window.add_child(close_btn)
	
	var min_btn := ColorRect.new()
	min_btn.color = Color(0.9, 0.8, 0.3, 0.8)
	min_btn.position = Vector2(26, 12)
	min_btn.size = Vector2(10, 10)
	window.add_child(min_btn)
	
	var max_btn := ColorRect.new()
	max_btn.color = Color(0.4, 0.9, 0.5, 0.8)
	max_btn.position = Vector2(42, 12)
	max_btn.size = Vector2(10, 10)
	window.add_child(max_btn)
	
	# Title text
	var title_label := Label.new()
	title_label.text = title
	title_label.position = Vector2(62, 8)
	title_label.add_theme_font_size_override("font_size", 14)
	title_label.add_theme_color_override("font_color", Color(0.7, 0.73, 0.8))
	window.add_child(title_label)
	
	return window


func _create_progress_bar(value: float, color: Color) -> Control:
	var container := Control.new()
	container.custom_minimum_size = Vector2(400, 20)
	
	var bg := ColorRect.new()
	bg.color = Color(0.15, 0.18, 0.22)
	bg.size = Vector2(400, 12)
	bg.position = Vector2(0, 4)
	container.add_child(bg)
	
	var fill := ColorRect.new()
	fill.color = color
	fill.size = Vector2(400 * value, 12)
	fill.position = Vector2(0, 4)
	container.add_child(fill)
	
	var label := Label.new()
	label.text = "%d%%" % int(value * 100)
	label.position = Vector2(410, 0)
	label.add_theme_font_size_override("font_size", 12)
	label.add_theme_color_override("font_color", color)
	container.add_child(label)
	
	return container


func _create_note_row(icon: String, text: String) -> Control:
	var row := HBoxContainer.new()
	row.custom_minimum_size.y = 36
	
	var icon_label := Label.new()
	icon_label.text = icon
	icon_label.add_theme_font_size_override("font_size", 18)
	icon_label.custom_minimum_size.x = 32
	row.add_child(icon_label)
	
	var text_label := Label.new()
	text_label.text = text
	text_label.add_theme_font_size_override("font_size", 14)
	text_label.add_theme_color_override("font_color", Color(0.8, 0.82, 0.88))
	row.add_child(text_label)
	
	return row


func _create_metric_row(label_text: String, value_text: String) -> Control:
	var row := HBoxContainer.new()
	row.custom_minimum_size.y = 28
	
	var label := Label.new()
	label.text = label_text + ":"
	label.add_theme_font_size_override("font_size", 13)
	label.add_theme_color_override("font_color", Color(0.55, 0.58, 0.65))
	label.custom_minimum_size.x = 140
	row.add_child(label)
	
	var value := Label.new()
	value.text = value_text
	value.add_theme_font_size_override("font_size", 13)
	value.add_theme_color_override("font_color", Color(0.75, 0.78, 0.85))
	row.add_child(value)
	
	return row


func _create_activity_row(time: String, event: String) -> Control:
	var row := HBoxContainer.new()
	row.custom_minimum_size.y = 32
	
	var time_label := Label.new()
	time_label.text = time
	time_label.add_theme_font_size_override("font_size", 12)
	time_label.add_theme_color_override("font_color", Color(0.4, 0.75, 0.9))
	time_label.custom_minimum_size.x = 50
	row.add_child(time_label)
	
	var event_label := Label.new()
	event_label.text = event
	event_label.add_theme_font_size_override("font_size", 13)
	event_label.add_theme_color_override("font_color", Color(0.7, 0.72, 0.78))
	event_label.autowrap_mode = TextServer.AUTOWRAP_OFF
	row.add_child(event_label)
	
	return row


func _on_ariaos_render_request(_ariaos_state: Dictionary) -> void:
	print("[OpticalMemory] ARIAOS render request received")
	# TODO: Parse ariaos_state and update widgets dynamically
	# For now, just re-render demo content
	_populate_ariaos_demo()
	
	_ariaos_viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
	
	await get_tree().process_frame
	await get_tree().process_frame
	
	var ariaos_image := _ariaos_viewport.get_texture().get_image()
	var ariaos_bytes := ariaos_image.save_png_to_buffer()
	
	if _bridge and _bridge.has_method("send_ariaos_image"):
		_bridge.send_ariaos_image(ariaos_bytes)


## Manually trigger ARIAOS render (for testing)
func render_ariaos_now() -> void:
	_on_ariaos_render_request({})


func _on_render_request(chat_history: Array, memory_nodes: Array) -> void:
	print("[OpticalMemory] Render request received: %d chat messages, %d memory nodes" % [chat_history.size(), memory_nodes.size()])
	_populate_memory_map(memory_nodes)
	_populate_chat_transcript(chat_history)
	_populate_status_panel(memory_nodes)
	
	# Request viewport updates
	_memory_viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
	_chat_viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
	_status_viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
	
	# Wait for viewports to render
	await get_tree().process_frame
	await get_tree().process_frame
	
	# Capture the rendered images
	var memory_image := _memory_viewport.get_texture().get_image()
	var chat_image := _chat_viewport.get_texture().get_image()
	var status_image := _status_viewport.get_texture().get_image()
	
	# Send to daemon
	var memory_bytes := memory_image.save_png_to_buffer()
	var chat_bytes := chat_image.save_png_to_buffer()
	var status_bytes := status_image.save_png_to_buffer()
	
	if _bridge:
		_bridge.send_rendered_images(memory_bytes, chat_bytes, status_bytes)


func _populate_memory_map(memory_nodes: Array) -> void:
	# Clear existing children
	for child in _memory_container.get_children():
		child.queue_free()
	
	if memory_nodes.is_empty():
		var empty_label := Label.new()
		empty_label.text = "(No memory nodes)"
		empty_label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
		empty_label.add_theme_font_size_override("font_size", 14)
		_memory_container.add_child(empty_label)
		return
	
	for node_data in memory_nodes:
		var node_panel := _create_memory_node_panel(node_data)
		_memory_container.add_child(node_panel)


func _create_memory_node_panel(node_data: Dictionary) -> Control:
	var panel := PanelContainer.new()
	var style := StyleBoxFlat.new()
	style.bg_color = Color(0.18, 0.22, 0.28, 1.0)
	style.corner_radius_top_left = 6
	style.corner_radius_top_right = 6
	style.corner_radius_bottom_left = 6
	style.corner_radius_bottom_right = 6
	style.content_margin_left = 12
	style.content_margin_right = 12
	style.content_margin_top = 8
	style.content_margin_bottom = 8
	panel.add_theme_stylebox_override("panel", style)
	
	var vbox := VBoxContainer.new()
	
	var label_text: String = str(node_data.get("label", "Unknown"))
	var weight: float = float(node_data.get("weight", 0.5))
	
	var title := Label.new()
	title.text = label_text
	title.add_theme_font_size_override("font_size", 15)
	title.add_theme_color_override("font_color", Color(0.8, 0.85, 0.95))
	vbox.add_child(title)
	
	# Weight bar
	var bar_bg := ColorRect.new()
	bar_bg.custom_minimum_size = Vector2(200, 8)
	bar_bg.color = Color(0.25, 0.28, 0.32)
	vbox.add_child(bar_bg)
	
	var bar_fill := ColorRect.new()
	bar_fill.custom_minimum_size = Vector2(200 * weight, 8)
	bar_fill.color = Color(0.4, 0.7, 0.9)
	bar_fill.position = Vector2.ZERO
	bar_bg.add_child(bar_fill)
	
	# Metadata summary if available
	var metadata: Dictionary = node_data.get("metadata", {}) as Dictionary
	if metadata.has("summary"):
		var summary := Label.new()
		summary.text = str(metadata.summary).substr(0, 80)
		if str(metadata.summary).length() > 80:
			summary.text += "..."
		summary.add_theme_font_size_override("font_size", 12)
		summary.add_theme_color_override("font_color", Color(0.6, 0.65, 0.7))
		summary.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		summary.custom_minimum_size.x = 440
		vbox.add_child(summary)
	
	panel.add_child(vbox)
	return panel


func _create_chat_message_panel(message: Dictionary) -> Control:
	var sender := str(message.get("sender", "unknown"))
	var content := str(message.get("content", ""))
	var is_user := sender.to_lower() == "user"
	
	# Memory tier for visual fade (Aria's "forgetting without amnesia")
	var tier := str(message.get("tier", "hot")).to_lower()
	var relevance := float(message.get("relevance", 1.0))
	
	# Calculate opacity based on tier
	var opacity: float
	match tier:
		"hot":
			opacity = 1.0
		"warm":
			opacity = 0.65
		"cold":
			opacity = 0.35
		_:
			opacity = relevance  # Fallback to raw relevance
	
	var hbox := HBoxContainer.new()
	hbox.custom_minimum_size.y = 0
	hbox.modulate.a = opacity  # Apply fade to entire message
	
	# Tier indicator (small colored dot)
	var tier_dot := ColorRect.new()
	tier_dot.custom_minimum_size = Vector2(4, 14)
	match tier:
		"hot":
			tier_dot.color = Color(0.3, 0.9, 0.4, opacity)  # Green
		"warm":
			tier_dot.color = Color(0.9, 0.7, 0.2, opacity)  # Yellow
		"cold":
			tier_dot.color = Color(0.5, 0.5, 0.5, opacity)  # Gray
		_:
			tier_dot.color = Color(0.5, 0.5, 0.5, opacity)
	hbox.add_child(tier_dot)
	
	# Small spacer
	var spacer := Control.new()
	spacer.custom_minimum_size.x = 6
	hbox.add_child(spacer)
	
	# Sender label
	var sender_label := Label.new()
	sender_label.text = sender.capitalize() + ":"
	sender_label.custom_minimum_size.x = 54
	sender_label.add_theme_font_size_override("font_size", 13)
	if is_user:
		sender_label.add_theme_color_override("font_color", Color(0.5, 0.7, 1.0))
	else:
		sender_label.add_theme_color_override("font_color", Color(0.95, 0.6, 0.75))
	hbox.add_child(sender_label)
	
	# Content
	var content_label := Label.new()
	# Truncate long messages
	if content.length() > 55:
		content_label.text = content.substr(0, 55) + "..."
	else:
		content_label.text = content
	content_label.add_theme_font_size_override("font_size", 13)
	content_label.add_theme_color_override("font_color", Color(0.85, 0.85, 0.85))
	content_label.autowrap_mode = TextServer.AUTOWRAP_OFF
	hbox.add_child(content_label)
	
	return hbox


func _populate_status_panel(memory_nodes: Array) -> void:
	# Clear existing children
	for child in _status_container.get_children():
		child.queue_free()
	
	# Show companion status - for now just Aria
	var aria_panel := _create_companion_status("Aria", "available", 0.0)
	_status_container.add_child(aria_panel)
	
	# Add some system status info
	var spacer := Control.new()
	spacer.custom_minimum_size.y = 20
	_status_container.add_child(spacer)
	
	var system_label := Label.new()
	system_label.text = "System Status"
	system_label.add_theme_font_size_override("font_size", 14)
	system_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
	_status_container.add_child(system_label)
	
	var memory_count := Label.new()
	memory_count.text = "Memory nodes: %d" % memory_nodes.size()
	memory_count.add_theme_font_size_override("font_size", 12)
	memory_count.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
	_status_container.add_child(memory_count)


func _populate_chat_transcript(chat_history: Array) -> void:
	# Clear existing children
	for child in _chat_container.get_children():
		child.queue_free()
	
	if chat_history.is_empty():
		var empty_label := Label.new()
		empty_label.text = "(No recent messages)"
		empty_label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
		empty_label.add_theme_font_size_override("font_size", 14)
		_chat_container.add_child(empty_label)
		return
	
	# Count tiers for display
	var hot_count := 0
	var warm_count := 0
	var cold_count := 0
	for msg in chat_history:
		var tier := str(msg.get("tier", "hot")).to_lower()
		match tier:
			"hot": hot_count += 1
			"warm": warm_count += 1
			"cold": cold_count += 1
	
	# Show tier summary at top
	var tier_summary := HBoxContainer.new()
	tier_summary.custom_minimum_size.y = 18
	
	var tier_label := Label.new()
	tier_label.text = "Memory: "
	tier_label.add_theme_font_size_override("font_size", 11)
	tier_label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
	tier_summary.add_child(tier_label)
	
	# Hot indicator
	var hot_dot := ColorRect.new()
	hot_dot.custom_minimum_size = Vector2(8, 8)
	hot_dot.color = Color(0.3, 0.9, 0.4)
	tier_summary.add_child(hot_dot)
	var hot_label := Label.new()
	hot_label.text = " %d  " % hot_count
	hot_label.add_theme_font_size_override("font_size", 11)
	hot_label.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
	tier_summary.add_child(hot_label)
	
	# Warm indicator
	var warm_dot := ColorRect.new()
	warm_dot.custom_minimum_size = Vector2(8, 8)
	warm_dot.color = Color(0.9, 0.7, 0.2)
	tier_summary.add_child(warm_dot)
	var warm_label := Label.new()
	warm_label.text = " %d  " % warm_count
	warm_label.add_theme_font_size_override("font_size", 11)
	warm_label.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
	tier_summary.add_child(warm_label)
	
	# Cold indicator
	var cold_dot := ColorRect.new()
	cold_dot.custom_minimum_size = Vector2(8, 8)
	cold_dot.color = Color(0.5, 0.5, 0.5)
	tier_summary.add_child(cold_dot)
	var cold_label := Label.new()
	cold_label.text = " %d" % cold_count
	cold_label.add_theme_font_size_override("font_size", 11)
	cold_label.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
	tier_summary.add_child(cold_label)
	
	_chat_container.add_child(tier_summary)
	
	# Add separator
	var separator := HSeparator.new()
	separator.custom_minimum_size.y = 4
	_chat_container.add_child(separator)
	
	# Show most recent messages (newest at bottom)
	var messages_to_show = chat_history.slice(0, 10)  # Limit to 10 messages (room for tier summary)
	for message in messages_to_show:
		var msg_panel := _create_chat_message_panel(message)
		_chat_container.add_child(msg_panel)


func _create_companion_status(companion_name: String, status: String, cooldown: float) -> Control:
	var panel := PanelContainer.new()
	var style := StyleBoxFlat.new()
	style.bg_color = Color(0.15, 0.18, 0.22, 1.0)
	style.corner_radius_top_left = 8
	style.corner_radius_top_right = 8
	style.corner_radius_bottom_left = 8
	style.corner_radius_bottom_right = 8
	style.content_margin_left = 16
	style.content_margin_right = 16
	style.content_margin_top = 12
	style.content_margin_bottom = 12
	panel.add_theme_stylebox_override("panel", style)
	
	var vbox := VBoxContainer.new()
	
	var name_label := Label.new()
	name_label.text = companion_name
	name_label.add_theme_font_size_override("font_size", 18)
	name_label.add_theme_color_override("font_color", Color(0.95, 0.6, 0.75))
	vbox.add_child(name_label)
	
	var status_hbox := HBoxContainer.new()
	
	var status_dot := ColorRect.new()
	status_dot.custom_minimum_size = Vector2(10, 10)
	if status == "available":
		status_dot.color = Color(0.3, 0.9, 0.4)
	elif status == "cooldown":
		status_dot.color = Color(0.9, 0.7, 0.2)
	else:
		status_dot.color = Color(0.5, 0.5, 0.5)
	status_hbox.add_child(status_dot)
	
	var status_label := Label.new()
	status_label.text = " " + status.capitalize()
	if cooldown > 0:
		status_label.text += " (%.1fs)" % cooldown
	status_label.add_theme_font_size_override("font_size", 14)
	status_label.add_theme_color_override("font_color", Color(0.7, 0.75, 0.8))
	status_hbox.add_child(status_label)
	
	vbox.add_child(status_hbox)
	panel.add_child(vbox)
	
	return panel
