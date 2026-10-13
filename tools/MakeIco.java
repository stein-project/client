/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

import java.awt.AlphaComposite;
import java.awt.Graphics2D;
import java.awt.RenderingHints;
import java.awt.image.BufferedImage;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.util.ArrayList;
import java.util.List;
import javax.imageio.ImageIO;

/**
 * PNG -> Windows .ico (PNG entries from 16 to 256 px), for the executable's icon.
 * Usage: java tools/MakeIco.java input.png output.ico
 */
public class MakeIco {

    public static void main(String[] a) throws Exception {
        BufferedImage src = ImageIO.read(new File(a[0]));
        int[] sizes = {16, 24, 32, 48, 64, 128, 256};
        List<byte[]> pngs = new ArrayList<>();
        for (int s : sizes) {
            ByteArrayOutputStream b = new ByteArrayOutputStream();
            ImageIO.write(scale(src, s), "png", b);
            pngs.add(b.toByteArray());
        }
        try (DataOutputStream out = new DataOutputStream(new FileOutputStream(a[1]))) {
            le16(out, 0);
            le16(out, 1);
            le16(out, sizes.length);
            int offset = 6 + 16 * sizes.length;
            for (int i = 0; i < sizes.length; i++) {
                out.writeByte(sizes[i] == 256 ? 0 : sizes[i]);
                out.writeByte(sizes[i] == 256 ? 0 : sizes[i]);
                out.writeByte(0);
                out.writeByte(0);
                le16(out, 1);
                le16(out, 32);
                le32(out, pngs.get(i).length);
                le32(out, offset);
                offset += pngs.get(i).length;
            }
            for (byte[] p : pngs) {
                out.write(p);
            }
        }
    }

    static BufferedImage scale(BufferedImage img, int size) {
        while (img.getWidth() / 2 >= size) {
            img = draw(img, img.getWidth() / 2);
        }
        return draw(img, size);
    }

    static BufferedImage draw(BufferedImage src, int size) {
        BufferedImage out = new BufferedImage(size, size, BufferedImage.TYPE_INT_ARGB);
        Graphics2D g = out.createGraphics();
        g.setRenderingHint(RenderingHints.KEY_INTERPOLATION, RenderingHints.VALUE_INTERPOLATION_BICUBIC);
        g.setComposite(AlphaComposite.Src);
        g.drawImage(src, 0, 0, size, size, null);
        g.dispose();
        return out;
    }

    static void le16(DataOutputStream o, int v) throws Exception {
        o.writeByte(v & 255);
        o.writeByte(v >> 8 & 255);
    }

    static void le32(DataOutputStream o, int v) throws Exception {
        le16(o, v & 0xFFFF);
        le16(o, v >>> 16);
    }
}
