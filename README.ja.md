# hyprPanopticon

[English README is here](README.md)

Hyprland のワークスペースオーバービュー。起動すると、Hyprland で開いているワークスペースが円周上に配置されて表示されます。フォーカス中のワークスペースは最大サイズで表示され、他のプレビューは全体のレイアウトが画面に収まるよう自動的に縮小されます。

プレビューは**ライブサムネイル**です: すべてのウィンドウは Hyprland の `hyprland-toplevel-export-v1` プロトコル経由でキャプチャされ(非表示のワークスペース上のウィンドウでも動作します)、実際のウィンドウジオメトリに従って合成されます。オーバーレイが開いている間、リングでフォーカスされているワークスペースは継続的に再キャプチャされ、Hyprland のイベント(ウィンドウの開閉・移動)によってオーバービュー全体がライブに更新されます。

## 使い方

`hyprpanopticon` を起動します(通常は Hyprland のキーバインドから)。オーバーレイはフォーカス中のモニタ全体を覆います。

| 入力 | 動作 |
|---|---|
| `←` `↑` / `h` `k` | フォーカスを反時計回りに回転 |
| `→` `↓` / `l` `j` | フォーカスを時計回りに回転 |
| マウスホイール | フォーカスを回転 |
| `Enter` / `Space` | フォーカス中のワークスペースに切り替えて閉じる |
| プレビューをクリック | そのワークスペースに切り替えて閉じる |
| `1`–`9` | 番号付きの special ワークスペースをトグル |
| `Esc` / `q` | 切り替えずに閉じる |

特別なワークスペース(スクラッチパッド)は、リングの外側の左端に番号付きの列として表示されます。クリックするか、対応する番号キーを押すと、そのワークスペースの表示/非表示を切り替えられます。モニタのビューポートを超えて広がるウィンドウ(例: スクロールレイアウト)は、モニタに表示される範囲でクリップされます。

## インストール(Nix)

インストールせずに試す:

```sh
nix run github:Uliboooo/hyprPanopticon
```

### flake の input として追加する(NixOS / home-manager など)

自分の flake の inputs に追加します:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    hyprpanopticon = {
      url = "github:Uliboooo/hyprPanopticon";
      # 任意: 自分の nixpkgs を共有して二重取得を避ける
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, hyprpanopticon, ... }: {
    # ... 自分の outputs
  };
}
```

あとはパッケージ一覧を組み立てる場所で `hyprpanopticon.packages.${system}.default` を参照します。home-manager の場合:

```nix
{ pkgs, inputs, ... }:
{
  home.packages = [
    inputs.hyprpanopticon.packages.${pkgs.system}.default
  ];
}
```

NixOS モジュールの場合:

```nix
{ pkgs, inputs, ... }:
{
  environment.systemPackages = [
    inputs.hyprpanopticon.packages.${pkgs.system}.default
  ];
}
```

対応システム: `x86_64-linux`、`aarch64-linux`。

Hyprland 設定の例:

```conf
bind = SUPER, Tab, exec, hyprpanopticon

# 任意: オーバーレイの背景にぼかしを適用
layerrule = blur, hyprPanopticon
layerrule = ignorealpha 0.2, hyprPanopticon
```

## 設定

任意設定ファイル `~/.config/hyprpanopticon/config.toml` で指定します。すべてのキーは任意であり、範囲外の値はクランプされます:

```toml
# フォーカスと反対側のプレビューのスケール(0.05..1.0、デフォルト 0.45)。
min_scale = 0.45
# フォーカスから離れるほどプレビューがどれだけ速く縮小するか(0.1..10、デフォルト 2.0)。
falloff = 2.0
# フォーカスされたプレビューの幅を画面幅に対する割合で指定(0.1..0.8、デフォルト 0.34)。
focus_width = 0.34
# 画面端からのマージン(ピクセル単位、0..200、デフォルト 24)。
margin = 24
# 角度方向の密度(0..1、デフォルト 0.7): 0 はプレビューを円周上に均等に配置し、値を大きくすると上部に余裕を持たせ、小さなプレビューを下部に密集させます。
spread = 0.7
# 小さなサイドプレビューを水平方向に中央へ引き寄せる度合い(0..1、デフォルト 0.4): 0 はすべてを一つの円周上に保ち(中央が空洞)、値を大きくすると中央を埋めつつ、上部と下部のプレビューはその位置に留まります。
center_pull = 0.4
# フォーカス中のモニターのワークスペースのみを表示する(デフォルト false:
# リングには全モニターのワークスペースが表示されます)。
per_monitor_workspaces = false
```

## ソースからビルド

```sh
nix develop        # rustc, cargo, GTK4, gtk4-layer-shell を含む開発シェル
cargo build
cargo test         # レイアウト計算のユニットテスト
```

デバッグ用ヘルパー: `hyprpanopticon --dump-window 0xADDR [out.png]` は、単一のトップレベル(`hyprctl clients` で取得したアドレス)を PNG としてキャプチャします。

## アーキテクチャ

- `src/layout.rs` — 純粋な円形レイアウトの計算(角度、コサインによるスケール減衰、半径フィッティング)。ユニットテスト済み。
- `src/ipc/` — Hyprland IPC: ワンショットのスナップショット(モニタ/ワークスペース/クライアント)と、ライブ更新をトリガーするイベントリスナー。
- `src/capture/` — 独自の Wayland 接続を持つワーカースレッドで `hyprland-toplevel-export-v1` を扱い、`wl_shm` バッファへの逐次キャプチャを行い、バイト列として UI に渡し、メインスレッドで `gdk::MemoryTexture` に変換します。
- `src/ui/` — GTK4 ウィジェット: レイヤーシェルオーバーレイウィンドウ、`RingView` コンテナ(円形レイアウト+回転アニメーション)、`WorkspacePreview`(ウィンドウテクスチャを合成し、ピクセルが到着するまでは色付き矩形でフォールバック)。
