// Sets or reads the user flags of a runtime permission of a package, as the shell (uid 2000) may: Android 9 has no `pm set-permission-flags`.
// Run on the device: app_process -Djava.class.path=/data/local/tmp/af_permflags.dex /system/bin PermFlags <package> <permission> get|fixed|clear
// `fixed` is what "Deny, and don't ask again" leaves (USER_SET and USER_FIXED): the next request shows no dialog.
import android.os.IBinder;
import java.lang.reflect.Method;

public class PermFlags {
    private static final int USER_SET = 1;
    private static final int USER_FIXED = 1 << 1;

    public static void main(String[] a) throws Exception {
        String pkg = a[0], perm = a[1], what = a[2];
        Object binder = Class.forName("android.os.ServiceManager").getMethod("getService", String.class).invoke(null, "package");
        Object pm = Class.forName("android.content.pm.IPackageManager$Stub").getMethod("asInterface", IBinder.class).invoke(null, binder);
        Class<?> iface = Class.forName("android.content.pm.IPackageManager");
        if (what.equals("fixed")) {
            iface.getMethod("updatePermissionFlags", String.class, String.class, int.class, int.class, int.class)
                    .invoke(pm, perm, pkg, USER_SET | USER_FIXED, USER_SET | USER_FIXED, 0);
        } else if (what.equals("clear")) {
            iface.getMethod("updatePermissionFlags", String.class, String.class, int.class, int.class, int.class)
                    .invoke(pm, perm, pkg, USER_SET | USER_FIXED, 0, 0);
        }
        Method get = iface.getMethod("getPermissionFlags", String.class, String.class, int.class);
        System.out.println("flags=" + get.invoke(pm, perm, pkg, 0));
    }
}
