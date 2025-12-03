extends Control
## Chat window for displaying conversation and handling user input

@onready var chat_scroll: ScrollContainer = $ChatScroll
@onready var chat_messages: VBoxContainer = $ChatScroll/ChatMessages
@onready var message_input: LineEdit = $InputContainer/MessageInput
@onready var send_button: Button = $InputContainer/SendButton

# Colors for different senders
const COLORS = {
	"user": Color(0.4, 0.6, 1.0),
	"aria": Color(0.9, 0.5, 0.7),
	"default": Color(0.7, 0.7, 0.7),
}


func _ready() -> void:
	# Connect UI signals
	send_button.pressed.connect(_on_send_pressed)
	message_input.text_submitted.connect(_on_text_submitted)


## Add a message to the chat display
func add_message(sender: String, content: String) -> void:
	var bubble = _create_bubble(sender, content)
	chat_messages.add_child(bubble)
	
	# Scroll to bottom after a frame
	await get_tree().process_frame
	chat_scroll.scroll_vertical = int(chat_scroll.get_v_scroll_bar().max_value)


func _create_bubble(sender: String, content: String) -> Control:
	var container = HBoxContainer.new()
	container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	
	# Determine alignment
	var is_user = sender.to_lower() == "user"
	
	# Small margin spacer on the alignment side (bubble stretches nearly full width)
	const MARGIN_SIZE = 24
	
	if is_user:
		# Add small spacer on left for right alignment
		var spacer = Control.new()
		spacer.custom_minimum_size.x = MARGIN_SIZE
		container.add_child(spacer)
	
	# Create the bubble panel - expands to fill available space
	var panel = PanelContainer.new()
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	
	# Style the bubble
	var style = StyleBoxFlat.new()
	style.bg_color = COLORS.get(sender.to_lower(), COLORS["default"])
	style.bg_color.a = 0.9
	style.corner_radius_top_left = 14
	style.corner_radius_top_right = 14
	style.corner_radius_bottom_left = 14 if is_user else 4
	style.corner_radius_bottom_right = 4 if is_user else 14
	style.content_margin_left = 16
	style.content_margin_right = 16
	style.content_margin_top = 12
	style.content_margin_bottom = 12
	panel.add_theme_stylebox_override("panel", style)
	
	# Create content container
	var vbox = VBoxContainer.new()
	vbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	
	# Add sender label (except for user)
	if not is_user:
		var sender_label = Label.new()
		sender_label.text = sender.capitalize()
		sender_label.add_theme_font_size_override("font_size", 24)
		sender_label.add_theme_color_override("font_color", Color(1, 1, 1, 0.8))
		vbox.add_child(sender_label)
	
	# Add message content - expands to fill bubble width
	var content_label = RichTextLabel.new()
	content_label.text = content
	content_label.fit_content = true
	content_label.bbcode_enabled = false
	content_label.scroll_active = false
	content_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	content_label.add_theme_font_size_override("normal_font_size", 24)
	content_label.add_theme_font_size_override("bold_font_size", 24)
	content_label.add_theme_color_override("default_color", Color.WHITE)
	vbox.add_child(content_label)
	
	panel.add_child(vbox)
	container.add_child(panel)
	
	if not is_user:
		# Add small spacer on right for left alignment
		var spacer = Control.new()
		spacer.custom_minimum_size.x = MARGIN_SIZE
		container.add_child(spacer)
	
	return container


func _on_send_pressed() -> void:
	_send_message()


func _on_text_submitted(_text: String) -> void:
	_send_message()


func _send_message() -> void:
	var text = message_input.text.strip_edges()
	if text.is_empty():
		return
	
	# Add to local display
	add_message("user", text)
	
	# Send to daemon
	DaemonBridge.send_user_message(text)
	
	# Clear input
	message_input.text = ""
	message_input.grab_focus()
