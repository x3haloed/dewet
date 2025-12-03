extends Node
## WebSocket bridge to the Dewet Daemon
##
## This autoload handles all communication between Godot and the Rust daemon.
## It emits signals when messages arrive and provides methods to send messages.

# Signals for incoming messages
signal connected
signal disconnected
signal speak_requested(character_id: String, text: String, audio: PackedByteArray, mood: String, urgency: float)
signal react_requested(character_id: String, expression: String)
signal render_optical_memory_requested(chat_history: Array, memory_nodes: Array)
signal render_ariaos_requested(ariaos_state: Dictionary)
signal ariaos_command_received(commands: Array)
signal screen_capture_received(image_base64: String, timestamp: int, active_window: String, active_app: String)
signal arbiter_decision_received(decision: Dictionary)
signal log_received(level: String, message: String, timestamp: int)

# Connection state
enum State { DISCONNECTED, CONNECTING, CONNECTED }
var _state: State = State.DISCONNECTED

# WebSocket
var _socket: WebSocketPeer = WebSocketPeer.new()
var _url: String = "ws://127.0.0.1:7777"
var _reconnect_timer: float = 0.0
var _reconnect_delay: float = 2.0

# Client identification
var _identified: bool = false


func _ready() -> void:
	print("[DaemonBridge] Initializing...")
	_connect_to_daemon()


func _process(delta: float) -> void:
	if _state == State.DISCONNECTED:
		_reconnect_timer += delta
		if _reconnect_timer >= _reconnect_delay:
			_reconnect_timer = 0.0
			_connect_to_daemon()
		return
	
	_socket.poll()
	var state = _socket.get_ready_state()
	
	match state:
		WebSocketPeer.STATE_OPEN:
			if _state == State.CONNECTING:
				_state = State.CONNECTED
				print("[DaemonBridge] Connected to daemon")
			
			# Process incoming messages
			while _socket.get_available_packet_count() > 0:
				var packet = _socket.get_packet()
				var text = packet.get_string_from_utf8()
				_handle_message(text)
		
		WebSocketPeer.STATE_CLOSING:
			pass  # Wait for close
		
		WebSocketPeer.STATE_CLOSED:
			var code = _socket.get_close_code()
			var reason = _socket.get_close_reason()
			print("[DaemonBridge] Disconnected: %d - %s" % [code, reason])
			_state = State.DISCONNECTED
			_identified = false
			disconnected.emit()


func _connect_to_daemon() -> void:
	print("[DaemonBridge] Connecting to %s..." % _url)
	_state = State.CONNECTING
	# Increase buffer size to handle large messages (composite images are ~1MB)
	_socket.inbound_buffer_size = 4 * 1024 * 1024  # 4MB
	_socket.outbound_buffer_size = 1024 * 1024     # 1MB
	var err = _socket.connect_to_url(_url)
	if err != OK:
		print("[DaemonBridge] Connection failed: %d" % err)
		_state = State.DISCONNECTED


func _handle_message(json_str: String) -> void:
	var msg = JSON.parse_string(json_str)
	if msg == null:
		push_warning("[DaemonBridge] Failed to parse message: %s" % json_str)
		return
	
	var msg_type = msg.get("type", "")
	
	match msg_type:
		"hello":
			_identified = true
			print("[DaemonBridge] Hello from daemon")
			connected.emit()
		
		"connected":
			_identified = true
			print("[DaemonBridge] Identified as: %s" % msg.get("client_type", "unknown"))
			connected.emit()
		
		"speak":
			var audio = PackedByteArray()
			if msg.has("audio_base64") and msg.audio_base64 != null:
				audio = Marshalls.base64_to_raw(msg.audio_base64)
			speak_requested.emit(
				msg.get("character_id", ""),
				msg.get("text", ""),
				audio,
				msg.get("mood", "neutral"),
				msg.get("urgency", 0.5)
			)
		
		"react":
			react_requested.emit(
				msg.get("character_id", ""),
				msg.get("expression", "")
			)
		
		"render_optical_memory":
			render_optical_memory_requested.emit(
				msg.get("chat_history", []),
				msg.get("memory_nodes", [])
			)
		
		"render_ariaos":
			render_ariaos_requested.emit(
				msg.get("ariaos_state", {})
			)
		
		"ariaos_command":
			ariaos_command_received.emit(
				msg.get("commands", [])
			)
		
		"screen_capture":
			screen_capture_received.emit(
				msg.get("image_base64", ""),
				msg.get("timestamp", 0),
				msg.get("active_window", ""),
				msg.get("active_app", "")
			)
		
		"arbiter_decision":
			arbiter_decision_received.emit(msg)
		
		"log":
			log_received.emit(
				msg.get("level", "info"),
				msg.get("message", ""),
				msg.get("timestamp", 0)
			)
		
		# Internal daemon messages - Godot doesn't need to act on these
		"observation_snapshot", "vision_analysis", "decision_update":
			pass
		
		_:
			print("[DaemonBridge] Unknown message type: %s" % msg_type)


## Send a user chat message to the daemon
func send_user_message(text: String) -> void:
	_send({"type": "user_chat", "text": text})


## Send rendered optical memory images back to daemon
func send_rendered_images(memory_png: PackedByteArray, chat_png: PackedByteArray, status_png: PackedByteArray) -> void:
	_send({
		"type": "optical_render_result",
		"memory": Marshalls.raw_to_base64(memory_png),
		"chat": Marshalls.raw_to_base64(chat_png),
		"status": Marshalls.raw_to_base64(status_png),
	})


## Send rendered ARIAOS image back to daemon
func send_ariaos_image(ariaos_png: PackedByteArray) -> void:
	_send({
		"type": "ariaos_render_result",
		"image": Marshalls.raw_to_base64(ariaos_png),
	})


## Force a character to speak (debug)
func force_speak(character_id: String, text: String = "") -> void:
	var msg = {"type": "force_speak", "character_id": character_id}
	if text != "":
		msg["text"] = text
	_send(msg)


## Reset all character cooldowns (debug)
func reset_cooldowns() -> void:
	_send({"type": "reset_cooldowns"})


## Request current state (debug)
func get_state() -> void:
	_send({"type": "get_state"})


## Execute ARIAOS DSL commands directly (debug/testing)
## Example: exec_dsl('ariaos.apps.notes.set_content("Hello world")')
func exec_dsl(dsl_text: String) -> void:
	_send({
		"type": "debug_command",
		"command": "exec_dsl",
		"payload": {"text": dsl_text}
	})


## Check if connected to daemon
func is_daemon_connected() -> bool:
	return _state == State.CONNECTED and _identified


func _send(data: Dictionary) -> void:
	if _socket.get_ready_state() == WebSocketPeer.STATE_OPEN:
		var json = JSON.stringify(data)
		_socket.send_text(json)

