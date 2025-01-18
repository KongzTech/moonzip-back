core.register_action("add_cors", { "http-res" }, function(applet)
	applet.http:res_add_header("Access-Control-Allow-Origin", "*")
	applet.http:res_add_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
	applet.http:res_add_header("Access-Control-Allow-Headers", "Content-Type, Authorization")
end)

core.register_service("ok_response", "http", function(applet)
	local response = ""
	applet:set_status(200)
	applet:add_header("Content-Length", "0")
	applet:start_response()
	applet:send(response)
end)

core.register_service("rate_limit", "http", function(applet)
	local response = ""
	applet:set_status(429)
	applet:add_header("Content-Length", "0")
	applet:start_response()
	applet:send(response)
end)