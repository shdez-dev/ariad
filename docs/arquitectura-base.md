# Ariadna: línea base de arquitectura

Estado: línea base inicial, 9 de noviembre de 2026. Esta decisión concreta las propuestas ADR del documento de Arquitectura y decisiones técnicas de AETHER. El prototipo no está listo para operación productiva.

## Decisiones vigentes

1. **Aplicación:** monolito modular de escritorio con Rust estable y Tauri v2.
2. **Interfaz:** plantilla Vanilla TypeScript; HTML, CSS y recursos compilados a archivos estáticos con Vite. Tauri empaqueta esos recursos.
3. **Comunicación:** las acciones de interfaz llamarán comandos IPC Tauri que delegan en casos de uso Rust. El esqueleto inicial aún no registra comandos.
4. **Persistencia:** SQLite local mediante SQLx, con migraciones SQL versionadas. Todavía no está implementada.
5. **Eventos:** el simulador correrá en proceso y usará los mismos casos de uso que la interfaz. No se usará API HTTP ni MQTT en esta etapa.
6. **Datos:** sintéticos y configurados para una bodega lógica. No hay lectores físicos, robótica ni mapeo físico automático.

## Desarrollo local

`src-tauri/tauri.conf.json` no declara `devUrl`. El hook de desarrollo ejecuta `npm run build`; Tauri CLI sirve los recursos estáticos de `dist` en modo desarrollo y el WebView puede mostrar un origen local. No hay servidor Vite separado ni API HTTP de negocio. Para publicación, `frontendDist` se empaqueta con la aplicación. La interfaz se comunica con el backend mediante IPC de Tauri.

Entorno verificado: Windows 11 Pro 25H2 (10.0.26200.9711, x64), Rust/Cargo 1.99.1 con `stable-x86_64-pc-windows-msvc`, Node.js 24.22.1, npm 11.19.1, Visual Studio Community 2022 con herramientas C++, Windows SDK 10.0.26100.0 y WebView2 Runtime 157.0.4341.42. Smart App Control bloqueó auxiliares de Cargo en el primer intento; cada equipo de desarrollo requiere una configuración de ejecución aprobada.

Comando de inicio:

```powershell
npm run tauri dev
```

## Límites de esta entrega

La interfaz TypeScript/Vite y la aplicación Tauri se compilaron correctamente. La ventana inicial se abrió en Windows x64 y mostró la pantalla de arranque. Smart App Control había bloqueado auxiliares de Cargo durante el primer intento; el entorno se ajustó para completar la compilación. Este resultado confirma solo el arranque de escritorio. No demuestra persistencia, inventario, mapa, recepción, traslados, despacho, simulación ni rendimiento.

