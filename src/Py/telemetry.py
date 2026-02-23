"""
 * Blur Auto Clicker - telemetry.py
 * Copyright (C) 2026  [Blur009]
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * any later version.
 *
 * Made with Spite. (the emotion)
 *
"""
import requests
import threading
from src.Private.Supabase import SUPABASE_URL, SUPABASE_KEY
from datetime import datetime, timezone

# Module-level state
_ui_widgets = None
_log_func = None
_current_version = None
_SUPABASE_URL = SUPABASE_URL
_SUPABASE_KEY = SUPABASE_KEY


def initialize(ui_widgets, log_func, current_version):
    # Initialize telemetry with UI widgets, logging, version, and URL.
    global _ui_widgets, _log_func, _current_version, _SUPABASE_URL, _SUPABASE_KEY
    _ui_widgets = ui_widgets
    _log_func = log_func
    _current_version = current_version


def send_telemetry_data():
    if not _ui_widgets or not _SUPABASE_URL or not _SUPABASE_KEY:
        return

    try:
        data = {
            "timestamp":                datetime.now(timezone.utc).isoformat(),
            "click_speed":              _ui_widgets.click_speed_input.value(),
            "click_interval":           _ui_widgets.click_interval_combobox.currentText(),
            "mouse_button":             _ui_widgets.mouse_button_combobox.currentText(),
            "duty_cycle":               _ui_widgets.duty_cycle_input.value(),
            "activation_type":          _ui_widgets.activation_type_combobox.currentText(),
            "speed_variation_enabled":  _ui_widgets.speed_variation_checkbox.isChecked(),
            "speed_variation_amount":   _ui_widgets.speed_variation_input.value(),
            "click_limit_enabled":      _ui_widgets.click_limit_checkbox.isChecked(),
            "click_limit_value":        _ui_widgets.click_limit_input.value(),
            "time_limit_enabled":       _ui_widgets.time_limit_checkbox.isChecked(),
            "time_limit_value":         _ui_widgets.time_limit_input.value(),
            "time_limit_unit":          _ui_widgets.time_limit_combobox.currentText(),
            "position_enabled":         _ui_widgets.position_options_checkbox.isChecked(),
            "offset_enabled":           _ui_widgets.click_offset_checkbox.isChecked(),
            "offset_value":             _ui_widgets.click_offset_input.value(),
            "offset_chance_enabled":    _ui_widgets.click_offset_chance_input_checkbox.isChecked(),
            "offset_chance_value":      _ui_widgets.click_offset_chance_input.value(),
            "smoothing_enabled":        _ui_widgets.click_offset_smoothing_checkbox.isChecked(),
            "advanced_used":            _ui_widgets.advanced_options_checkbox.isChecked(),
            "version":                  _current_version,
        }

        headers = {
            "apikey": _SUPABASE_KEY,
            "Authorization": f"Bearer {_SUPABASE_KEY}",
            "Content-Type": "application/json",
            "Prefer": "return=minimal"
        }

        response = requests.post(
            _SUPABASE_URL + "/rest/v1/Main", json=data, headers=headers, timeout=5)
        if _log_func:
            _log_func(
                f"Telemetry response: {response.status_code} - {response.text}")

    except Exception as e:
        if _log_func:
            _log_func(f"Telemetry failed: {e}")


def send_telemetry():
    # Send telemetry in a background thread if enabled.
    if not _SUPABASE_URL or not _ui_widgets:
        return
    if not _ui_widgets.telemetry_checkbox.isChecked():
        return
    threading.Thread(target=send_telemetry_data, daemon=True).start()
