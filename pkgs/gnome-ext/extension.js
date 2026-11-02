import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {QuickMenuToggle, SystemIndicator} from 'resource:///org/gnome/shell/ui/quickSettings.js';

const NAME = 'io.github.kvmon.Monitor';
const PATH = '/io/github/kvmon/Monitor';

const KvmToggle = GObject.registerClass(class KvmToggle extends QuickMenuToggle {
    _init() {
        super._init({title: 'KVM', iconName: 'video-display-symbolic', toggleMode: false});
        this.menu.setHeader('video-display-symbolic', 'KVM');
        this.connect('clicked', () => this.menu.open());
    }
});

export default class KvmExtension extends Extension {
    enable() {
        this._cancel = new Gio.Cancellable();
        this._quitting = false;
        this._indicator = new SystemIndicator();
        this._toggle = new KvmToggle();
        this._indicator.quickSettingsItems.push(this._toggle);
        Main.panel.statusArea.quickSettings.addExternalIndicator(this._indicator);
        this._showError('Checking monitor…');
        this._toggle.menu.connect('open-state-changed', (_menu, open) => {
            if (open && this._proxy?.g_name_owner)
                this._call('Refresh');
        });
        const [, bytes] = this.dir.get_child('interface.xml').load_contents(null);
        const Proxy = Gio.DBusProxy.makeProxyWrapper(new TextDecoder().decode(bytes));
        const cancel = this._cancel;
        new Proxy(Gio.DBus.session, NAME, PATH, (proxy, error) => {
            if (cancel.is_cancelled())
                return;
            this._proxy = proxy;
            if (error) {
                this._showError(`Monitor service unavailable: ${error.message}`);
                return;
            }
            this._changed = proxy.connectSignal('Changed', (_proxy, _sender, state) => this._render(state));
            this._owner = proxy.connect('notify::g-name-owner', () => {
                if (proxy.g_name_owner)
                    this._call('GetState', null, state => this._render(state));
                else if (!this._quitting)
                    this._showError('Monitor service unavailable');
            });
            this._call('GetState', null, state => this._render(state));
        }, cancel);
    }

    _call(method, parameters = null, done = null) {
        const cancel = this._cancel;
        this._proxy.call(method, parameters, Gio.DBusCallFlags.NONE, 5000, cancel, (proxy, result) => {
            if (cancel.is_cancelled())
                return;
            try {
                const reply = proxy.call_finish(result).deepUnpack();
                done?.(reply);
            } catch (error) {
                if (!this._quitting)
                    this._showError(error.message);
            }
        });
    }

    _showError(message) {
        this._render(['unavailable', message, 0, [], 'Quit']);
    }

    _render([status, message, active, computers, quitLabel]) {
        this._toggle.subtitle = status === 'ready' ? message : 'Unavailable';
        this._toggle.checked = status === 'ready';
        this._toggle.menu.removeAll();
        if (status !== 'ready' || !computers.some(([, input]) => input === active)) {
            const item = new PopupMenu.PopupMenuItem(message, {reactive: false});
            this._toggle.menu.addMenuItem(item);
            if (status === 'checking' || status === 'switching')
                this._toggle.subtitle = message;
        }
        computers.sort((a, b) => Number(b[1] === active) - Number(a[1] === active));
        for (const [label, input] of computers) {
            const item = new PopupMenu.PopupMenuItem(label);
            item.setOrnament(status === 'ready' && input === active
                ? PopupMenu.Ornament.CHECK : PopupMenu.Ornament.NONE);
            item.setSensitive(status === 'ready');
            item.connect('activate', () => {
                if (input !== active)
                    this._call('Select', new GLib.Variant('(u)', [input]));
            });
            this._toggle.menu.addMenuItem(item);
        }
        this._toggle.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        const quit = new PopupMenu.PopupMenuItem(quitLabel);
        quit.connect('activate', () => {
            this._quitting = true;
            // Wait for the reply before disable() cancels outstanding calls.
            const cancel = this._cancel;
            const finish = () => Main.extensionManager.disableExtension(this.uuid);
            if (this._proxy?.g_name_owner) {
                this._proxy.call('Quit', null, Gio.DBusCallFlags.NO_AUTO_START,
                    5000, cancel, (proxy, result) => {
                        try {
                            proxy.call_finish(result);
                        } catch (error) {
                            console.warn(`KVM Quit: ${error.message}`);
                        }
                        if (!cancel.is_cancelled())
                            finish();
                    });
            } else {
                finish();
            }
        });
        this._toggle.menu.addMenuItem(quit);
    }

    disable() {
        this._cancel.cancel();
        if (this._changed)
            this._proxy.disconnectSignal(this._changed);
        if (this._owner)
            this._proxy.disconnect(this._owner);
        this._changed = 0;
        this._owner = 0;
        this._proxy = null;
        this._toggle.destroy();
        this._indicator.destroy();
        this._toggle = null;
        this._indicator = null;
    }
}
