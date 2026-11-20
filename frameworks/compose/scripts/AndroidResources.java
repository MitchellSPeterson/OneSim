import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Base64;
import java.util.HexFormat;
import java.util.List;
import java.util.Locale;
import java.util.function.Predicate;
import java.util.regex.Pattern;
import javax.imageio.ImageIO;
import javax.xml.XMLConstants;
import javax.xml.parsers.DocumentBuilderFactory;
import org.w3c.dom.Element;

/** Generates Android resources for the source-only Compose guests. */
class AndroidResources {
    private static final String ANDROID = "http://schemas.android.com/apk/res/android";
    private static final Pattern NAME = Pattern.compile("[a-z][a-z0-9_]*");

    public static void main(String[] args) throws Exception {
        Path app = Path.of(args[0]).toAbsolutePath();
        if (!Files.isDirectory(app)) app = app.getParent();
        String namespace = args[1];
        Path output = Path.of(args[2]);
        for (String folder : List.of("res", "src/main/res", "app/src/main/res")) {
            Path root = app.resolve(folder);
            List<Path> fonts = resources(root, "font", AndroidResources::supported);
            List<Path> drawables = resources(root, "drawable", AndroidResources::supportedImage);
            List<Path> nodpi = resources(root, "drawable-nodpi", AndroidResources::supportedImage);
            var images = new java.util.LinkedHashMap<String, Path>();
            for (Path asset : drawables) images.put(resourceName(asset), asset);
            for (Path asset : nodpi) images.put(resourceName(asset), asset);
            if (fonts.isEmpty() && images.isEmpty()) continue;

            Path manifest = root.getParent().resolve("AndroidManifest.xml");
            if (Files.isRegularFile(manifest)) {
                String value = xml(manifest).getAttribute("package");
                if (!value.isEmpty()) namespace = value;
            }
            var generated = new StringBuilder();
            if (!namespace.isEmpty()) generated.append("package ").append(namespace).append("\n");
            generated.append("import onesim.compose.Guest\nobject R {\n");

            if (!fonts.isEmpty()) {
                generated.append("  object font {\n");
                var names = fonts.stream().map(AndroidResources::resourceName).toList();
                if (names.stream().distinct().count() != names.size()) {
                    throw new IllegalArgumentException("Duplicate font resource in " + root);
                }
                for (int i = 0; i < fonts.size(); i++) {
                    Path path = fonts.get(i);
                    int id = i + 1;
                    generated.append("    val `").append(names.get(i)).append("`: Int by lazy {\n");
                    if (path.toString().endsWith(".xml")) {
                        writeFamily(generated, path, id, names, fonts);
                    } else {
                        writeFace(generated, path, id);
                    }
                    generated.append("    }\n");
                }
                generated.append("  }\n");
            }
            if (!images.isEmpty()) {
                generated.append("  object drawable {\n");
                int id = 1;
                for (var image : images.entrySet()) {
                    generated.append("    val `").append(image.getKey()).append("`: Int by lazy {\n");
                    writeImage(generated, image.getValue(), id++);
                    generated.append("    }\n");
                }
                generated.append("  }\n");
            }
            Files.writeString(output, generated.append("}\n"));
            break;
        }
    }

    private static List<Path> resources(Path root, String folder, Predicate<Path> supported) throws Exception {
        Path path = root.resolve(folder);
        if (!Files.isDirectory(path)) return List.of();
        try (var files = Files.list(path)) {
            return files.filter(Files::isRegularFile).filter(supported).sorted().toList();
        }
    }

    private static String resourceName(Path path) {
        String file = path.getFileName().toString();
        String name = file.substring(0, file.lastIndexOf('.'));
        if (!NAME.matcher(name).matches()) throw new IllegalArgumentException("Invalid resource: " + path);
        return name;
    }

    private static boolean supportedImage(Path file) {
        String name = file.getFileName().toString();
        return (name.endsWith(".png") && !name.endsWith(".9.png")) || name.endsWith(".jpg") || name.endsWith(".jpeg");
    }

    private static void writeImage(StringBuilder code, Path path, int id) throws Exception {
        var image = ImageIO.read(path.toFile());
        if (image == null) throw new IllegalArgumentException("Invalid image resource: " + path);
        int width = image.getWidth();
        int height = image.getHeight();
        var data = ByteBuffer.allocate(Math.multiplyExact(Math.multiplyExact(width, height), 4));
        for (int pixel : image.getRGB(0, 0, width, height, null, 0, width)) data.putInt(pixel);
        code.append("      Guest.registerImageResource(").append(id).append(", ").append(width).append(", ")
            .append(height).append(", ").append(image.getColorModel().hasAlpha()).append(", ");
        writePayload(code, data.array());
    }

    private static boolean supported(Path file) {
        String name = file.getFileName().toString();
        return name.endsWith(".ttf") || name.endsWith(".otf") || name.endsWith(".ttc") || name.endsWith(".xml");
    }

    private static Element xml(Path path) throws Exception {
        var factory = DocumentBuilderFactory.newInstance();
        factory.setNamespaceAware(true);
        factory.setFeature("http://apache.org/xml/features/disallow-doctype-decl", true);
        factory.setAttribute(XMLConstants.ACCESS_EXTERNAL_DTD, "");
        factory.setAttribute(XMLConstants.ACCESS_EXTERNAL_SCHEMA, "");
        return factory.newDocumentBuilder().parse(path.toFile()).getDocumentElement();
    }

    private static void writeFace(StringBuilder code, Path path, int id) throws Exception {
        byte[] data = Files.readAllBytes(path);
        String digest = HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(data));
        code.append("      Guest.registerFontResource(").append(id).append(", \"").append(digest).append("\", ");
        writePayload(code, data);
    }

    private static void writePayload(StringBuilder code, byte[] data) {
        code.append("buildString {\n");
        String encoded = Base64.getEncoder().encodeToString(data);
        for (int start = 0; start < encoded.length(); start += 2048) {
            int end = Math.min(start + 2048, encoded.length());
            code.append("        append(\"").append(encoded, start, end).append("\")\n");
        }
        code.append("      })\n");
    }

    private static void writeFamily(StringBuilder code, Path path, int id, List<String> resources, List<Path> paths) throws Exception {
        Element family = xml(path);
        if (!family.getTagName().equals("font-family")) {
            throw new IllegalArgumentException("Expected <font-family> in " + path);
        }
        var members = new ArrayList<String>();
        var children = family.getChildNodes();
        for (int i = 0; i < children.getLength(); i++) {
            if (!(children.item(i) instanceof Element child) || !child.getTagName().equals("font")) continue;
            String reference = attribute(child, "font");
            String name = reference.startsWith("@font/") ? reference.substring(6) : "";
            int index = resources.indexOf(name);
            if (index < 0 || paths.get(index).toString().endsWith(".xml")) {
                throw new IllegalArgumentException("Missing local font face " + reference + " in " + path);
            }
            int weight = parse(attribute(child, "fontWeight"), 400);
            int ttcIndex = parse(attribute(child, "ttcIndex"), 0);
            boolean italic = attribute(child, "fontStyle").toLowerCase(Locale.ROOT).equals("italic");
            members.add("Guest.FontResourceFace(`" + name + "`, " + weight + ", " + italic + ", " + ttcIndex + ")");
        }
        if (members.isEmpty()) {
            String fallback = attribute(family, "fontProviderSystemFontFamily");
            if (fallback.isEmpty()) fallback = "sans-serif";
            if (!fallback.matches("[A-Za-z0-9_-]+")) {
                throw new IllegalArgumentException("Invalid system fallback in " + path);
            }
            code.append("    Guest.registerFontProviderFallback(").append(id).append(", \"").append(fallback).append("\")\n");
        } else {
            code.append("    Guest.registerFontFamilyResource(").append(id).append(", listOf(")
                .append(String.join(", ", members)).append("))\n");
        }
    }

    private static int parse(String value, int fallback) {
        return value.isEmpty() ? fallback : Integer.parseInt(value);
    }

    private static String attribute(Element element, String name) {
        String value = element.getAttributeNS(ANDROID, name);
        if (!value.isEmpty()) return value;
        var attributes = element.getAttributes();
        for (int i = 0; i < attributes.getLength(); i++) {
            var entry = attributes.item(i);
            if (name.equals(entry.getLocalName())) return entry.getNodeValue();
        }
        return "";
    }
}
