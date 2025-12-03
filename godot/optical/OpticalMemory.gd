extends Control
## Renders optical memory visualizations for the VLM composite image
##
## Uses SubViewports to render proper text that the VLM can read.

@export var bridge_path: NodePath = NodePath("/root/DaemonBridge")

const PANEL_SIZE := Vector2i(512, 512)

var _bridge: Node

# Viewports for rendering each panel
var _memory_viewport: SubViewport
var _chat_viewport: SubViewport
var _status_viewport: SubViewport

# UI elements in each viewport
var _memory_container: VBoxContainer
var _chat_container: VBoxContainer
var _status_container: VBoxContainer


func _ready() -> void:
	_setup_viewports()
	
	if bridge_path != NodePath(""):
		_bridge = get_node_or_null(bridge_path)
	if _bridge:
		_bridge.render_optical_memory_requested.connect(_on_render_request)


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
