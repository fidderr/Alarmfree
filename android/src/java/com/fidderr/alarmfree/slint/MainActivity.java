package com.fidderr.alarmfree.slint;

import android.app.NativeActivity;
import android.os.Build;
import android.os.Bundle;
import android.view.Display;
import android.view.WindowManager;

/**
 * Hosts the UI, which Slint draws from libmain.so (named in the manifest). Everything
 * else Android needs is the SDK's.
 *
 * A native window runs at the display's default refresh rate unless it asks for more
 * (a WebView page is given the faster rate by the system itself), so the activity asks for
 * the fastest mode the display has at its current size.
 */
public class MainActivity extends NativeActivity {
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        askForTheFastestMode();
    }

    @Override
    protected void onResume() {
        super.onResume();
        askForTheFastestMode();
    }

    private void askForTheFastestMode() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.M) {
            return;
        }
        try {
            Display display = getWindowManager().getDefaultDisplay();
            Display.Mode current = display.getMode();
            Display.Mode best = current;
            for (Display.Mode mode : display.getSupportedModes()) {
                if (mode.getPhysicalWidth() == current.getPhysicalWidth()
                        && mode.getPhysicalHeight() == current.getPhysicalHeight()
                        && mode.getRefreshRate() > best.getRefreshRate()) {
                    best = mode;
                }
            }
            WindowManager.LayoutParams params = getWindow().getAttributes();
            if (params.preferredDisplayModeId != best.getModeId()) {
                params.preferredDisplayModeId = best.getModeId();
                getWindow().setAttributes(params);
            }
        } catch (RuntimeException e) {
            // Staying at the default rate is only slower, never wrong.
        }
    }
}
