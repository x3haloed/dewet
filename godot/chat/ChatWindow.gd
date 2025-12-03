extends Control
## Chat window for displaying conversation and handling user input

@onready var chat_scroll: ScrollContainer = $ChatScroll
@onready var chat_messages: VBoxContainer = $ChatScroll/ChatMessages
@onready var message_input: LineEdit = $InputContainer/MessageInput
@onready var send_button: Button = $InputContainer/SendButton

# Message scene for chat bubbles
var _message_scene: PackedScene

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
	
	# Load or create message scene
	_create_message_scene()


func _create_message_scene() -> void:
	# We'll create bubbles dynamically instead of loading a scene
	pass


## Add a message to the chat display
func add_message(sender: String, content: String) -> void:
	var bubble = _create_bubble(sender, content)
	chat_messages.add_child(bubble)
	
	# Scroll to bottom after a frame
	await get_tree().process_frame
	chat_scroll.scroll_vertical = chat_scroll.get_v_scroll_bar().max_value


func _create_bubble(sender: String, content: String) -> Control:
	var container = HBoxContainer.new()
	container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	
	# Determine alignment
	var is_user = sender.to_lower() == "user"
	
	if is_user:
		# Add spacer on left for right alignment
		var spacer = Control.new()
		spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		spacer.custom_minimum_size.x = 50
		container.add_child(spacer)
	
	# Create the bubble panel
	var panel = PanelContainer.new()
	panel.size_flags_horizontal = Control.SIZE_SHRINK_END if is_user else Control.SIZE_SHRINK_BEGIN
	
	# Style the bubble
	var style = StyleBoxFlat.new()
	style.bg_color = COLORS.get(sender.to_lower(), COLORS["default"])
	style.bg_color.a = 0.9
	style.corner_radius_top_left = 12
	style.corner_radius_top_right = 12
	style.corner_radius_bottom_left = 12 if is_user else 4
	style.corner_radius_bottom_right = 4 if is_user else 12
	style.content_margin_left = 12
	style.content_margin_right = 12
	style.content_margin_top = 8
	style.content_margin_bottom = 8
	panel.add_theme_stylebox_override("panel", style)
	
	# Create content container
	var vbox = VBoxContainer.new()
	vbox.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	
	# Add sender label (except for user)
	if not is_user:
		var sender_label = Label.new()
		sender_label.text = sender
		sender_label.add_theme_font_size_override("font_size", 11)
		sender_label.add_theme_color_override("font_color", Color(1, 1, 1, 0.6))
		vbox.add_child(sender_label)
	
	# Add message content
	var content_label = RichTextLabel.new()
	content_label.text = content
	content_label.fit_content = true
	content_label.bbcode_enabled = false
	content_label.scroll_active = false
	content_label.custom_minimum_size.x = 100
	content_label.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	content_label.add_theme_color_override("default_color", Color.WHITE)
	vbox.add_child(content_label)
	
	panel.add_child(vbox)
	container.add_child(panel)
	
	if not is_user:
		# Add spacer on right for left alignment
		var spacer = Control.new()
		spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		spacer.custom_minimum_size.x = 50
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

