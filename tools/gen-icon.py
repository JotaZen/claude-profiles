"""Genera el logo de Claude Profiles: una C de cubos, en pixel art.

El dibujo se compone sobre una rejilla lógica de 32x32 y luego se escala con
vecino más cercano, así que los bordes siguen siendo píxeles nítidos en
cualquier tamaño. Paleta de Thaumware (violetas sobre #0f1115).

Uso:  python tools/gen-icon.py
Sale: icons/icon.png (1024) y icons/icon.ico (16..256)
"""

from pathlib import Path

from PIL import Image, ImageDraw

RAIZ = Path(__file__).resolve().parent.parent
ICONOS = RAIZ / "icons"

LADO = 32  # rejilla lógica
ESCALA = 32  # 32 * 32 = 1024 px

FONDO = (15, 17, 21)  # --color-page de Thaumware
BORDE = (30, 27, 75)  # añil profundo

# Un tono por fila de cubos: del lila claro al violeta profundo.
FRENTES = [
    (167, 139, 250),
    (139, 92, 246),
    (124, 58, 237),
    (109, 40, 217),
    (91, 33, 182),
]

# La C: cuatro columnas por cinco filas, con las esquinas izquierdas vacías
# para que el trazo se lea curvo y no como una U girada.
PATRON = [
    ".XXX",
    "X...",
    "X...",
    "X...",
    ".XXX",
]

CUBO = 5  # lado del cubo en píxeles lógicos
SEPARACION = 0  # los cubos se tocan: el canto oscuro ya los separa


def mezclar(color, otro, factor):
    return tuple(round(c + (o - c) * factor) for c, o in zip(color, otro))


def dibujar_cubo(lienzo, x, y, frente):
    """Un cubo chato: cara superior clara, frente plano, canto y base oscuros."""
    superior = mezclar(frente, (255, 255, 255), 0.40)
    canto = mezclar(frente, BORDE, 0.60)
    sombra = mezclar(frente, BORDE, 0.78)
    brillo = mezclar(frente, (255, 255, 255), 0.75)

    lienzo.rectangle([x, y, x + CUBO - 1, y + CUBO - 1], fill=frente)
    lienzo.rectangle([x, y, x + CUBO - 1, y], fill=superior)  # cara de arriba
    lienzo.rectangle([x + CUBO - 1, y + 1, x + CUBO - 1, y + CUBO - 1], fill=canto)
    lienzo.rectangle([x, y + CUBO - 1, x + CUBO - 2, y + CUBO - 1], fill=sombra)
    lienzo.point((x, y), fill=brillo)  # chispa de luz


def componer():
    img = Image.new("RGBA", (LADO, LADO), (0, 0, 0, 0))
    lienzo = ImageDraw.Draw(img)

    # Placa de fondo con esquinas redondeadas y un hilo de borde añil.
    lienzo.rounded_rectangle([0, 0, LADO - 1, LADO - 1], radius=6, fill=FONDO, outline=BORDE)

    filas = len(PATRON)
    columnas = len(PATRON[0])
    ancho = columnas * CUBO + (columnas - 1) * SEPARACION
    alto = filas * CUBO + (filas - 1) * SEPARACION
    x0 = (LADO - ancho) // 2
    y0 = (LADO - alto) // 2

    for fila, linea in enumerate(PATRON):
        for columna, celda in enumerate(linea):
            if celda != "X":
                continue
            dibujar_cubo(
                lienzo,
                x0 + columna * (CUBO + SEPARACION),
                y0 + fila * (CUBO + SEPARACION),
                FRENTES[fila],
            )

    return img


def exportar_svg(base, destino):
    """Vuelca la rejilla como rectángulos: escala sin perder el filo del píxel."""
    pixeles = base.convert("RGBA").load()
    partes = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {LADO} {LADO}" '
        f'width="{LADO}" height="{LADO}" shape-rendering="crispEdges">'
    ]
    for y in range(LADO):
        x = 0
        while x < LADO:
            color = pixeles[x, y]
            if color[3] == 0:
                x += 1
                continue
            # Agrupa píxeles contiguos del mismo color en un solo rect.
            ancho = 1
            while x + ancho < LADO and pixeles[x + ancho, y] == color:
                ancho += 1
            r, g, b, _ = color
            partes.append(
                f'<rect x="{x}" y="{y}" width="{ancho}" height="1" fill="#{r:02x}{g:02x}{b:02x}"/>'
            )
            x += ancho
    partes.append("</svg>")
    destino.write_text("\n".join(partes) + "\n", encoding="utf-8")


def main():
    base = componer()
    ICONOS.mkdir(exist_ok=True)

    grande = base.resize((LADO * ESCALA, LADO * ESCALA), Image.NEAREST)
    grande.save(ICONOS / "icon.png")

    # El .ico se arma desde reescalados exactos para que no se emborrone.
    tamanos = [16, 24, 32, 48, 64, 128, 256]
    capas = [base.resize((t, t), Image.NEAREST) for t in tamanos]
    capas[-1].save(ICONOS / "icon.ico", format="ICO", sizes=[(t, t) for t in tamanos])

    exportar_svg(base, RAIZ / "ui" / "logo.svg")

    print(f"icon.png  {grande.width}x{grande.height}")
    print(f"icon.ico  {', '.join(str(t) for t in tamanos)}")
    print("ui/logo.svg")


if __name__ == "__main__":
    main()
