# Forge — Concepto y Visión

## El Problema No Es la Inferencia Distribuida

Proyectos como [mesh-llm](https://github.com/Mesh-LLM/mesh-llm), Petals y Exo han demostrado que se puede dividir la inferencia de LLM en múltiples dispositivos a través de una red. La ingeniería difícil del paralelismo de pipeline, el sharding de expertos y la coordinación de la red está en gran medida resuelta.

El problema sin resolver es: **¿por qué alguien contribuiría con su hardware?**

mesh-llm agrupa GPUs maravillosamente, pero si ejecutas tu Mac Mini como un nodo mesh durante un año, no obtienes nada. Sin registro de contribución, sin acceso prioritario, sin retorno económico. La red funciona por buena voluntad. La buena voluntad no escala.

## La Idea: El Cómputo es Dinero

Cada sistema monetario está respaldado por la escasez. El oro es escaso debido a la geología. El petróleo es escaso porque su extracción cuesta energía. Bitcoin es escaso porque la minería quema electricidad en hashes SHA-256.

Pero la escasez de Bitcoin es artificial: la computación no tiene propósito. Los hashes aseguran el libro contable pero no producen nada útil.

La inferencia de LLM es diferente. Cuando un nodo de Forge gasta electricidad para responder a la pregunta de alguien, esa computación tiene un **valor intrínseco**. Alguien quería esa respuesta lo suficiente como para solicitarla. La electricidad no se desperdició — produjo inteligencia.

```
Bitcoin:   electricidad → hash inútil → escasez artificial → valor
Forge:     electricidad → inferencia útil → utilidad real → valor
```

Este es el **Estándar de Cómputo (計算本位制)**: un sistema monetario donde la unidad de valor está respaldada por una computación útil verificada.

## Qué es Forge

Forge es mesh-llm con una economía.

La capa de inferencia (redes, distribución de modelos, API) proviene de mesh-llm. Forge añade:

1. **Libro Contable de CU** — Cada inferencia crea una transacción. El proveedor gana CU, el consumidor gasta CU. Firmado doblemente por ambas partes.
2. **Precios Dinámicos** — El CU por token fluctúa con la oferta y demanda local. Más nodos inactivos → más barato. Más solicitudes → más caro.
3. **Prueba de Trabajo Útil** — El CU se gana realizando inferencias reales, no resolviendo acertijos arbitrarios.
4. **API de Presupuesto del Agente** — Los agentes de IA pueden consultar su saldo, estimar costos y tomar decisiones de gasto autónomas.
5. **Puentes Externos** — El CU puede intercambiarse opcionalmente por Bitcoin (Lightning), stablecoins o fiat a través de capas de adaptadores fuera del protocolo.

## ¿Por qué no usar simplemente Bitcoin?

Consideramos hacer de Bitcoin/Lightning la capa de liquidación principal. Decidimos no hacerlo.

| Preocupación | Explicación |
|---------|-------------|
| **Inconsistencia filosófica** | Recompensar el trabajo útil en una moneda respaldada por trabajo inútil. |
| **Dependencia externa** | Si la seguridad de Bitcoin falla (computación cuántica, regulatoria), la economía de Forge también falla. |
| **Eficiencia** | La gestión de canales Lightning es una sobrecarga para los micropagos por inferencia. |
| **Autosuficiencia** | El CU tiene valor porque la computación en sí misma es útil — no necesita validación externa. |

Bitcoin sigue estando disponible como una **rampa de salida** para los operadores que necesitan liquidez externa. Pero la economía nativa del protocolo funciona con CU.

## Por qué el CU tiene valor

El CU no es un token especulativo. Es un **derecho sobre el cómputo futuro**.

Si ganaste 10,000 CU sirviendo inferencias, puedes gastar esos CU para comprar inferencias de cualquier otro nodo de la red. El valor no es abstracto — es la capacidad de hacer que una máquina piense por ti.

Esto convierte al CU en un **activo productivo**, no en una reserva de valor:

```
Edificio de apartamentos        Mac Mini en Forge
───────────────────         ──────────────────
Activo: edificio             Activo: hardware de cómputo
Costo: mantenimiento         Costo: electricidad
Ingresos: alquiler           Ingresos: CU de inferencia
Rendimiento: alq. - mant.    Rendimiento: CU ganado - electricidad
Inactivo = ingresos perdidos Inactivo = potencial desperdiciado
```

A diferencia de Bitcoin (oro digital — mantiene el valor pero no produce nada), el CU es como una propiedad de alquiler — genera rendimiento al realizar un trabajo útil.

## Los Agentes de IA como Actores Económicos

El consumidor más importante de la economía de Forge no son los humanos, sino los agentes de IA.

Un agente que ejecuta un modelo local pequeño (1.5 mil millones de parámetros en un teléfono) tiene una inteligencia limitada. Pero si puede ganar CU prestando cómputo inactivo y gastar CU para acceder a modelos más grandes, puede expandir de forma autónoma sus propias capacidades:

```
Agente pequeño (teléfono, 1.5B)
  → inactivo durante la noche → presta CPU → gana CU
  → mañana: necesita razonamiento complejo
  → consulta /v1/forge/balance → tiene 5,000 CU
  → consulta /v1/forge/pricing → modelo 70B cuesta 2,000 CU por 500 tokens
  → compra inferencia 70B → obtiene una respuesta más inteligente
  → usa la respuesta para tomar mejores decisiones comerciales
  → gana más CU en el siguiente ciclo
```

Este es el bucle de autorrefuerzo: los agentes que toman buenas decisiones económicas se vuelven más fuertes, lo que les permite tomar decisiones aún mejores.

Ningún humano necesita aprobar transacciones individuales. El agente opera dentro de una política presupuestaria establecida por su propietario. El protocolo proporciona el mercado; el agente proporciona la estrategia.

## Comparación

| Proyecto | Inferencia | Economía | Autonomía del Agente |
|---------|-----------|---------|----------------|
| **mesh-llm** | Distribuida (pipeline + MoE) | Ninguna | Solo mensajería de pizarra (blackboard) |
| **Petals** | Distribuida (colaborativa) | Ninguna | Ninguna |
| **Ollama** | Solo local | Ninguna | Ninguna |
| **Together AI** | Centralizada | Pago por token (corporativo) | Solo acceso a la API |
| **Bitcoin** | N/A | PoW (trabajo inútil) | Ninguna |
| **Golem** | Cómputo por lotes | Token GNT | Dirigido por humanos |
| **Forge** | Distribuida (mesh-llm) | **CU (trabajo útil)** | **Gestión de presupuesto autónoma** |

## La Metáfora

Una semilla cae en la red. Gana sus primeros CU prestando ciclos inactivos durante la noche. Con esos CU, compra acceso a un modelo más grande. Se vuelve más inteligente. Encuentra transacciones más eficientes. Más CU. Un modelo más grande. Un bosque emerge de una sola semilla — no porque alguien la plantara, sino porque la economía hizo que el crecimiento fuera inevitable.
