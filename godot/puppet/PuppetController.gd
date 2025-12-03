extends Node2D
## Puppet controller for character avatar display and animation
##
## This is a simplified 2D puppet system. Can be extended to support:
## - Live2D via GDCubism
## - 3D models
## - More complex sprite animations

# Current state
var _current_expression: String = "neutral"
var _is_speaking: bool = false
var _urgency: float = 0.5

# Animation parameters
var _idle_time: float = 0.0
var _blink_timer: float = 0.0
var _blink_interval: float = 3.0
var _speak_bounce: float = 0.0

# Child nodes (will be created if not present)
var _sprite: Sprite2D
var _speech_indicator: Node2D


func _ready() -> void:
	_setup_visuals()


func _setup_visuals() -> void:
	# Create a simple placeholder avatar
	# In production, this would load actual character sprites/models
	
	_sprite = Sprite2D.new()
	_sprite.position = Vector2(200, 100)  # Center in puppet area
	add_child(_sprite)
	
	# Create a placeholder texture (colored circle)
	var img = Image.create(128, 128, false, Image.FORMAT_RGBA8)
	img.fill(Color(0.9, 0.5, 0.7, 1.0))  # Pink/coral color
	
	# Draw a simple face
	_draw_face(img, "neutral")
	
	var tex = ImageTexture.create_from_image(img)
	_sprite.texture = tex
	
	# Create speech indicator (three dots)
	_speech_indicator = Node2D.new()
	_speech_indicator.position = Vector2(200, 170)
	_speech_indicator.visible = false
	add_child(_speech_indicator)
	
	for i in range(3):
		var dot = Sprite2D.new()
		var dot_img = Image.create(8, 8, false, Image.FORMAT_RGBA8)
		dot_img.fill(Color.WHITE)
		dot.texture = ImageTexture.create_from_image(dot_img)
		dot.position = Vector2((i - 1) * 16, 0)
		_speech_indicator.add_child(dot)


func _draw_face(img: Image, expression: String) -> void:
	# Draw a circular face
	var center = Vector2(64, 64)
	var radius = 60
	
	# Fill circle
	for x in range(128):
		for y in range(128):
			var dist = Vector2(x, y).distance_to(center)
			if dist < radius:
				img.set_pixel(x, y, Color(0.9, 0.5, 0.7, 1.0))
			elif dist < radius + 2:
				img.set_pixel(x, y, Color(0.7, 0.3, 0.5, 1.0))  # Border
	
	# Draw eyes based on expression
	var eye_y = 50
	var eye_spacing = 20
	
	match expression:
		"happy", "excited":
			# Happy eyes (curved lines)
			_draw_arc(img, Vector2(center.x - eye_spacing, eye_y), 8, Color.BLACK)
			_draw_arc(img, Vector2(center.x + eye_spacing, eye_y), 8, Color.BLACK)
		"sad":
			# Sad eyes
			_draw_circle(img, Vector2(center.x - eye_spacing, eye_y), 6, Color.BLACK)
			_draw_circle(img, Vector2(center.x + eye_spacing, eye_y), 6, Color.BLACK)
		"thinking":
			# One eye closed
			_draw_circle(img, Vector2(center.x - eye_spacing, eye_y), 6, Color.BLACK)
			_draw_line(img, Vector2(center.x + eye_spacing - 5, eye_y), Vector2(center.x + eye_spacing + 5, eye_y), Color.BLACK)
		_:
			# Neutral eyes
			_draw_circle(img, Vector2(center.x - eye_spacing, eye_y), 6, Color.BLACK)
			_draw_circle(img, Vector2(center.x + eye_spacing, eye_y), 6, Color.BLACK)
	
	# Draw mouth based on expression
	var mouth_y = 80
	
	match expression:
		"happy", "excited":
			_draw_smile(img, Vector2(center.x, mouth_y), 15, Color.BLACK)
		"sad":
			_draw_frown(img, Vector2(center.x, mouth_y), 15, Color.BLACK)
		"thinking":
			_draw_line(img, Vector2(center.x - 8, mouth_y), Vector2(center.x + 8, mouth_y), Color.BLACK)
		_:
			_draw_line(img, Vector2(center.x - 10, mouth_y), Vector2(center.x + 10, mouth_y), Color.BLACK)


func _draw_circle(img: Image, center: Vector2, radius: int, color: Color) -> void:
	for x in range(int(center.x) - radius, int(center.x) + radius + 1):
		for y in range(int(center.y) - radius, int(center.y) + radius + 1):
			if Vector2(x, y).distance_to(center) <= radius:
				if x >= 0 and x < img.get_width() and y >= 0 and y < img.get_height():
					img.set_pixel(x, y, color)


func _draw_arc(img: Image, center: Vector2, radius: int, color: Color) -> void:
	# Draw a happy eye arc
	for x in range(int(center.x) - radius, int(center.x) + radius + 1):
		var dx = x - center.x
		var dy = -sqrt(max(0, radius * radius - dx * dx)) * 0.5
		var y = int(center.y + dy)
		if x >= 0 and x < img.get_width() and y >= 0 and y < img.get_height():
			img.set_pixel(x, y, color)
			if y + 1 < img.get_height():
				img.set_pixel(x, y + 1, color)


func _draw_line(img: Image, start: Vector2, end: Vector2, color: Color) -> void:
	var steps = int(start.distance_to(end))
	for i in range(steps + 1):
		var t = float(i) / float(steps) if steps > 0 else 0.0
		var pos = start.lerp(end, t)
		var x = int(pos.x)
		var y = int(pos.y)
		if x >= 0 and x < img.get_width() and y >= 0 and y < img.get_height():
			img.set_pixel(x, y, color)
			if y + 1 < img.get_height():
				img.set_pixel(x, y + 1, color)


func _draw_smile(img: Image, center: Vector2, width: int, color: Color) -> void:
	for x in range(int(center.x) - width, int(center.x) + width + 1):
		var dx = x - center.x
		var dy = (dx * dx) / float(width * width) * 8
		var y = int(center.y + dy)
		if x >= 0 and x < img.get_width() and y >= 0 and y < img.get_height():
			img.set_pixel(x, y, color)


func _draw_frown(img: Image, center: Vector2, width: int, color: Color) -> void:
	for x in range(int(center.x) - width, int(center.x) + width + 1):
		var dx = x - center.x
		var dy = -(dx * dx) / float(width * width) * 5 + 5
		var y = int(center.y + dy)
		if x >= 0 and x < img.get_width() and y >= 0 and y < img.get_height():
			img.set_pixel(x, y, color)


func _process(delta: float) -> void:
	_idle_time += delta
	
	# Idle bobbing
	if _sprite:
		var bob = sin(_idle_time * 2.0) * 3.0
		_sprite.position.y = 100 + bob
	
	# Speaking animation
	if _is_speaking:
		_speak_bounce += delta * 10.0
		if _speech_indicator:
			for i in _speech_indicator.get_child_count():
				var dot = _speech_indicator.get_child(i) as Sprite2D
				if dot:
					dot.position.y = sin(_speak_bounce + i * 0.5) * 5.0
	
	# Blinking
	_blink_timer += delta
	if _blink_timer >= _blink_interval:
		_blink_timer = 0.0
		_blink_interval = randf_range(2.0, 5.0)
		_trigger_blink()


func _trigger_blink() -> void:
	# Quick scale animation for blink effect
	if _sprite:
		var tween = create_tween()
		tween.tween_property(_sprite, "scale:y", 0.9, 0.05)
		tween.tween_property(_sprite, "scale:y", 1.0, 0.05)


## Set the character's expression
func set_expression(mood: String, urgency: float = 0.5) -> void:
	_current_expression = mood
	_urgency = urgency
	
	# Update the face
	_update_expression()


func _update_expression() -> void:
	if not _sprite:
		return
	
	var img = Image.create(128, 128, false, Image.FORMAT_RGBA8)
	img.fill(Color(0.9, 0.5, 0.7, 1.0))
	_draw_face(img, _current_expression)
	
	_sprite.texture = ImageTexture.create_from_image(img)


## Start speaking animation
func start_speaking() -> void:
	_is_speaking = true
	if _speech_indicator:
		_speech_indicator.visible = true
	
	# Increase animation energy based on urgency
	var tween = create_tween()
	tween.tween_property(_sprite, "scale", Vector2(1.05, 1.05), 0.1)


## Stop speaking animation
func stop_speaking() -> void:
	_is_speaking = false
	if _speech_indicator:
		_speech_indicator.visible = false
	
	var tween = create_tween()
	tween.tween_property(_sprite, "scale", Vector2(1.0, 1.0), 0.2)


## Play a reaction animation
func play_reaction(reaction: String) -> void:
	match reaction:
		"surprise":
			var tween = create_tween()
			tween.tween_property(_sprite, "scale", Vector2(1.2, 1.2), 0.1)
			tween.tween_property(_sprite, "scale", Vector2(1.0, 1.0), 0.3)
		"nod":
			var tween = create_tween()
			tween.tween_property(_sprite, "position:y", 90.0, 0.15)
			tween.tween_property(_sprite, "position:y", 110.0, 0.15)
			tween.tween_property(_sprite, "position:y", 100.0, 0.1)
		"shake":
			var tween = create_tween()
			tween.tween_property(_sprite, "position:x", 195.0, 0.05)
			tween.tween_property(_sprite, "position:x", 205.0, 0.05)
			tween.tween_property(_sprite, "position:x", 195.0, 0.05)
			tween.tween_property(_sprite, "position:x", 200.0, 0.05)
		_:
			pass

