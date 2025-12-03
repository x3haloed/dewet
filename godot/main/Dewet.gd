extends Control
## Main Dewet window controller
##
## Coordinates between the daemon bridge, chat UI, and puppet system.

@onready var puppet_controller: Node2D = $MainContainer/PuppetArea/PuppetController
@onready var chat_window: Control = $MainContainer/ChatWindow
@onready var audio_player: AudioStreamPlayer = $AudioStreamPlayer


func _ready() -> void:
	# Connect to daemon bridge signals
	DaemonBridge.connected.connect(_on_daemon_connected)
	DaemonBridge.disconnected.connect(_on_daemon_disconnected)
	DaemonBridge.speak_requested.connect(_on_speak_requested)
	DaemonBridge.react_requested.connect(_on_react_requested)
	
	# Set up window
	_setup_window()
	
	print("[Dewet] Ready")


func _setup_window() -> void:
	# Make window always on top and transparent
	get_window().always_on_top = true
	get_window().transparent = true
	get_window().transparent_bg = true


func _on_daemon_connected() -> void:
	print("[Dewet] Connected to daemon")
	# Could show a connection indicator


func _on_daemon_disconnected() -> void:
	print("[Dewet] Disconnected from daemon")
	# Could show a reconnecting indicator


func _on_speak_requested(character_id: String, text: String, audio: PackedByteArray, mood: String, urgency: float) -> void:
	print("[Dewet] Speak: %s says '%s' (mood: %s, urgency: %.2f)" % [character_id, text, mood, urgency])
	
	# Display the message in chat
	chat_window.add_message(character_id, text)
	
	# Trigger puppet animation based on mood/urgency
	puppet_controller.set_expression(mood, urgency)
	
	# Play audio if provided
	if audio.size() > 0:
		_play_audio(audio)
	
	# Start speaking animation
	puppet_controller.start_speaking()
	
	# Schedule end of speaking (estimate based on text length)
	var duration = max(2.0, text.length() * 0.05)
	await get_tree().create_timer(duration).timeout
	puppet_controller.stop_speaking()


func _on_react_requested(character_id: String, expression: String) -> void:
	print("[Dewet] React: %s -> %s" % [character_id, expression])
	puppet_controller.play_reaction(expression)


func _play_audio(audio_data: PackedByteArray) -> void:
	# Try to load and play the audio
	# This assumes AIFF format from macOS say command
	# In production, you'd want proper format detection
	
	# For now, just log that we got audio
	print("[Dewet] Audio received: %d bytes" % audio_data.size())
	
	# TODO: Convert audio format and play
	# var stream = AudioStreamWAV.new()
	# stream.data = audio_data
	# audio_player.stream = stream
	# audio_player.play()


func _input(event: InputEvent) -> void:
	# Handle global shortcuts
	if event.is_action_pressed("toggle_visibility"):
		visible = !visible

