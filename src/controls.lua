-- Hover controls for unbloatedtube's embedded player, drawn by mpv itself (the video is a
-- separate native window, so the app can't draw over it). Moving the pointer over the video
-- shows a bar like YouTube's: a seek line, then play/pause, previous/next, mute + volume and the
-- time on the left, and -10 s/+10 s, speed, picture-in-picture and fullscreen on the right. It
-- hides after a moment. What only the app can do (previous/next video, speed, picture-in-picture)
-- is passed to it through the user-data/unbloated/action property.
-- Clicks elsewhere on the video keep their meaning: one click pauses, a double click toggles
-- fullscreen.
local mp = require 'mp'
local options = require 'mp.options'

local opts = { accent = "454EFF" } -- ASS colour, BBGGRR
options.read_options(opts, "unbloated-controls")

local overlay = mp.create_osd_overlay("ass-events")
local visible = false
local hide_timer
local L = {} -- layout of the current frame: named rectangles {x1, y1, x2, y2}

local LEFT = { "play", "prev", "next", "mute", "vol", "time" }
local RIGHT = { "back10", "fwd10", "speed", "pip", "full" }
-- What goes first when the video is too narrow for everything.
local DROP = { "back10", "fwd10", "pip", "prev", "speed", "time", "vol" }

local function i(v) return math.floor(v + 0.5) end

local function layout()
    local w, h = mp.get_osd_size()
    if not w or w < 120 or h < 120 then return false end
    local u = math.max(1, h / 540)
    local bar, m = i(46 * u), i(12 * u)
    local ty = h - bar
    local size = function(name)
        if name == "vol" then return i(96 * u) end
        if name == "time" then return i(138 * u) end
        if name == "speed" then return i(60 * u) end
        return bar
    end
    local present = {}
    for _, n in ipairs(LEFT) do present[n] = true end
    for _, n in ipairs(RIGHT) do present[n] = true end
    local function total()
        local t = 0
        for n in pairs(present) do t = t + size(n) end
        return t
    end
    local d = 1
    while total() > w - 2 * m and d <= #DROP do
        present[DROP[d]] = nil
        d = d + 1
    end
    L = { w = w, h = h, u = u, bar = bar, m = m, ty = ty }
    L.panel = { 0, ty - i(26 * u), w, h }
    L.seek = { m, ty - i(18 * u), w - m, ty + i(2 * u) }
    local x = m
    for _, n in ipairs(LEFT) do
        if present[n] then
            L[n] = { x, ty, x + size(n), h }
            x = x + size(n)
        end
    end
    x = w - m
    for k = #RIGHT, 1, -1 do
        local n = RIGHT[k]
        if present[n] then
            L[n] = { x - size(n), ty, x, h }
            x = x - size(n)
        end
    end
    return true
end

local function inside(r, x, y) return r and x >= r[1] and x <= r[3] and y >= r[2] and y <= r[4] end

-- Filled polygon from a flat list of points.
local function poly(color, alpha, pts)
    local d = { "m", i(pts[1]), i(pts[2]) }
    for k = 3, #pts, 2 do
        d[#d + 1] = "l"
        d[#d + 1] = i(pts[k])
        d[#d + 1] = i(pts[k + 1])
    end
    return string.format("{\\an7\\pos(0,0)\\bord0\\shad0\\1c&H%s&\\1a&H%02X&\\p1}%s{\\p0}", color, alpha, table.concat(d, " "))
end

local function rect(color, alpha, x1, y1, x2, y2)
    return poly(color, alpha, { x1, y1, x2, y1, x2, y2, x1, y2 })
end

local function clock(sec)
    sec = math.max(0, math.floor(sec or 0))
    local h, m, s = math.floor(sec / 3600), math.floor(sec % 3600 / 60), sec % 60
    if h > 0 then return string.format("%d:%02d:%02d", h, m, s) end
    return string.format("%d:%02d", m, s)
end

local function center(r) return (r[1] + r[3]) / 2, (r[2] + r[4]) / 2 end

local function render()
    if not visible or not layout() then return end
    local u, bar, m, ty, w, h = L.u, L.bar, L.m, L.ty, L.w, L.h
    local paused = mp.get_property_bool("pause")
    local pos = mp.get_property_number("time-pos") or 0
    local dur = mp.get_property_number("duration") or 0
    local vol = mp.get_property_number("volume") or 100
    local muted = mp.get_property_bool("mute")
    local speed = mp.get_property_number("speed") or 1
    local white, grey = "FFFFFF", "A0A0A0"
    local s = bar * 0.5
    local out = {}
    local function add(str) out[#out + 1] = str end

    -- shade behind the controls, darker towards the bottom
    add(rect("000000", 0xC0, 0, L.panel[2], w, ty + i(bar * 0.4)))
    add(rect("000000", 0x90, 0, ty + i(bar * 0.4), w, h))

    -- seek line
    local sy1, sy2 = ty - i(8 * u), ty - i(5 * u)
    add(rect(grey, 0x70, m, sy1, w - m, sy2))
    if dur > 0 then
        local x = m + (w - 2 * m) * math.min(1, pos / dur)
        add(rect(opts.accent, 0x00, m, sy1, x, sy2))
        add(rect(opts.accent, 0x00, x - i(3 * u), sy1 - i(2 * u), x + i(3 * u), sy2 + i(2 * u)))
    end

    local cx, cy
    if L.play then
        cx, cy = center(L.play)
        if paused then
            add(poly(white, 0, { cx - s * 0.3, cy - s * 0.45, cx - s * 0.3, cy + s * 0.45, cx + s * 0.45, cy }))
        else
            add(rect(white, 0, cx - s * 0.32, cy - s * 0.42, cx - s * 0.08, cy + s * 0.42))
            add(rect(white, 0, cx + s * 0.08, cy - s * 0.42, cx + s * 0.32, cy + s * 0.42))
        end
    end
    if L.prev then
        cx, cy = center(L.prev)
        add(rect(white, 0, cx - s * 0.4, cy - s * 0.4, cx - s * 0.3, cy + s * 0.4))
        add(poly(white, 0, { cx + s * 0.4, cy - s * 0.4, cx + s * 0.4, cy + s * 0.4, cx - s * 0.25, cy }))
    end
    if L.next then
        cx, cy = center(L.next)
        add(poly(white, 0, { cx - s * 0.4, cy - s * 0.4, cx - s * 0.4, cy + s * 0.4, cx + s * 0.25, cy }))
        add(rect(white, 0, cx + s * 0.3, cy - s * 0.4, cx + s * 0.4, cy + s * 0.4))
    end

    -- speaker (with a cross when muted), then the volume slider
    if L.mute then
        cx, cy = center(L.mute)
        add(poly(white, 0, { cx - s * 0.45, cy - s * 0.18, cx - s * 0.22, cy - s * 0.18, cx + s * 0.06, cy - s * 0.45,
            cx + s * 0.06, cy + s * 0.45, cx - s * 0.22, cy + s * 0.18, cx - s * 0.45, cy + s * 0.18 }))
        if muted or vol <= 0 then
            local t = s * 0.06
            add(poly(white, 0, { cx + s * 0.18 - t, cy - s * 0.2, cx + s * 0.18 + t, cy - s * 0.2 - t, cx + s * 0.5 + t, cy + s * 0.2,
                cx + s * 0.5 - t, cy + s * 0.2 + t }))
            add(poly(white, 0, { cx + s * 0.5 - t, cy - s * 0.2 - t, cx + s * 0.5 + t, cy - s * 0.2, cx + s * 0.18 + t, cy + s * 0.2 + t,
                cx + s * 0.18 - t, cy + s * 0.2 }))
        else
            add(rect(white, 0, cx + s * 0.18, cy - s * 0.16, cx + s * 0.24, cy + s * 0.16))
            add(rect(white, 0, cx + s * 0.32, cy - s * 0.3, cx + s * 0.38, cy + s * 0.3))
        end
    end
    if L.vol then
        local vx1, vx2 = L.vol[1] + i(10 * u), L.vol[3] - i(10 * u)
        local vy = (L.vol[2] + L.vol[4]) / 2
        add(rect(grey, 0x70, vx1, vy - i(2 * u), vx2, vy + i(2 * u)))
        local fill = vx1 + (vx2 - vx1) * (muted and 0 or math.min(100, vol)) / 100
        add(rect(white, 0, vx1, vy - i(2 * u), fill, vy + i(2 * u)))
        add(rect(white, 0, fill - i(3 * u), vy - i(6 * u), fill + i(3 * u), vy + i(6 * u)))
    end
    if L.time then
        local _, ty2 = center(L.time)
        add(string.format("{\\an4\\pos(%d,%d)\\bord0\\shad1\\fs%d\\1c&H%s&}%s / %s", L.time[1] + i(6 * u), i(ty2), i(17 * u), white, clock(pos), clock(dur)))
    end

    -- right side: -10 s, +10 s, speed, picture-in-picture, fullscreen
    if L.back10 then
        cx, cy = center(L.back10)
        add(poly(white, 0, { cx + s * 0.05, cy - s * 0.38, cx + s * 0.05, cy + s * 0.38, cx - s * 0.4, cy }))
        add(poly(white, 0, { cx + s * 0.5, cy - s * 0.38, cx + s * 0.5, cy + s * 0.38, cx + s * 0.05, cy }))
    end
    if L.fwd10 then
        cx, cy = center(L.fwd10)
        add(poly(white, 0, { cx - s * 0.5, cy - s * 0.38, cx - s * 0.5, cy + s * 0.38, cx - s * 0.05, cy }))
        add(poly(white, 0, { cx - s * 0.05, cy - s * 0.38, cx - s * 0.05, cy + s * 0.38, cx + s * 0.4, cy }))
    end
    if L.speed then
        cx, cy = center(L.speed)
        add(string.format("{\\an5\\pos(%d,%d)\\bord0\\shad1\\fs%d\\1c&H%s&}%s", i(cx), i(cy), i(18 * u), white, string.format("%g×", speed)))
    end
    if L.pip then
        cx, cy = center(L.pip)
        local a, b, t = s * 0.5, s * 0.36, s * 0.07
        add(rect(white, 0, cx - a, cy - b, cx + a, cy - b + t))
        add(rect(white, 0, cx - a, cy + b - t, cx + a, cy + b))
        add(rect(white, 0, cx - a, cy - b, cx - a + t, cy + b))
        add(rect(white, 0, cx + a - t, cy - b, cx + a, cy + b))
        add(rect(white, 0, cx + a * 0.05, cy + b * 0.05, cx + a - t * 2, cy + b - t * 2))
    end
    if L.full then
        cx, cy = center(L.full)
        local a, len, t = s * 0.42, s * 0.28, s * 0.09
        for _, d in ipairs({ { -1, -1 }, { 1, -1 }, { -1, 1 }, { 1, 1 } }) do
            local px, py = cx + d[1] * a, cy + d[2] * a
            add(rect(white, 0, math.min(px, px - d[1] * len), math.min(py, py - d[2] * t), math.max(px, px - d[1] * len), math.max(py, py - d[2] * t)))
            add(rect(white, 0, math.min(px, px - d[1] * t), math.min(py, py - d[2] * len), math.max(px, px - d[1] * t), math.max(py, py - d[2] * len)))
        end
    end

    overlay.res_x, overlay.res_y = w, h
    overlay.data = table.concat(out, "\n")
    overlay:update()
end

local function hide()
    visible = false
    overlay:remove()
end

local function show()
    visible = true
    render()
    if hide_timer then hide_timer:kill() end
    local x, y = mp.get_mouse_pos()
    -- stays while the pointer is on the bar itself
    if layout() and not inside(L.panel, x, y) then
        hide_timer = mp.add_timeout(2.5, hide)
    end
end

mp.observe_property("mouse-pos", "native", function(_, p)
    if p and p.hover then
        show()
    elseif visible then
        if hide_timer then hide_timer:kill() end
        hide_timer = mp.add_timeout(0.4, hide)
    end
end)

for _, name in ipairs({ "pause", "volume", "mute", "fullscreen", "duration", "speed" }) do
    mp.observe_property(name, nil, function() if visible then render() end end)
end
mp.add_periodic_timer(0.25, function() if visible then render() end end)

local function on_bar(x, y) return visible and layout() and inside(L.panel, x, y) end
local function ask_app(what) mp.set_property("user-data/unbloated/action", what) end

mp.add_forced_key_binding("MBTN_LEFT", "unbloated-click", function()
    local x, y = mp.get_mouse_pos()
    if not on_bar(x, y) then
        mp.commandv("cycle", "pause")
    elseif inside(L.play, x, y) then
        mp.commandv("cycle", "pause")
    elseif inside(L.prev, x, y) then
        ask_app("prev")
    elseif inside(L.next, x, y) then
        ask_app("next")
    elseif inside(L.mute, x, y) then
        mp.commandv("cycle", "mute")
    elseif inside(L.vol, x, y) then
        local vx1, vx2 = L.vol[1] + i(10 * L.u), L.vol[3] - i(10 * L.u)
        mp.set_property_native("mute", false)
        mp.set_property_number("volume", math.max(0, math.min(100, (x - vx1) / (vx2 - vx1) * 100)))
    elseif inside(L.back10, x, y) then
        mp.commandv("seek", -10)
    elseif inside(L.fwd10, x, y) then
        mp.commandv("seek", 10)
    elseif inside(L.speed, x, y) then
        ask_app("speed")
    elseif inside(L.pip, x, y) then
        ask_app("pip")
    elseif inside(L.full, x, y) then
        mp.commandv("cycle", "fullscreen")
    elseif inside(L.seek, x, y) then
        mp.commandv("seek", math.max(0, math.min(100, (x - L.m) / (L.w - 2 * L.m) * 100)), "absolute-percent+exact")
    end
    if visible then show() end
end)

-- Right click: the app opens its video menu where the pointer is (x y in this window's pixels).
mp.add_forced_key_binding("MBTN_RIGHT", "unbloated-menu", function()
  local x, y = mp.get_mouse_pos()
  ask_app(string.format("menu %d %d", x, y))
end)

mp.add_forced_key_binding("MBTN_LEFT_DBL", "unbloated-dblclick", function()
    local x, y = mp.get_mouse_pos()
    if not on_bar(x, y) then mp.commandv("cycle", "fullscreen") end
end)
