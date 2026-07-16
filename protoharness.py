import json
import os
import sys
import traceback
from typing import List
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


def configure_utf8_runtime() -> None:
    """Configure les entrées/sorties texte en UTF-8, notamment sous Windows."""
    os.environ.setdefault("PYTHONUTF8", "1")
    os.environ.setdefault("PYTHONIOENCODING", "utf-8")

    for stream in (sys.stdin, sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if reconfigure is not None:
            reconfigure(encoding="utf-8", errors="replace")

# ============================================
# 1. CONFIGURATION ET PARAMÈTRES (Config)
# ============================================

class AgentConfig:
    """
    Gestionnaire de configuration minimaliste pour les connexions LLM.
    Tout est centralisé ici pour une maintenance facile et une fiabilité accrue.
    """
    def __init__(self, base_url: str = "http://localhost:11434", default_model: str = ""):
        self.base_url = base_url.rstrip("/")
        # NOTE: Le modèle est critique pour la performance de l'agent.
        self.default_model = default_model

    def get_ollama_url(self) -> str:
        return f"{self.base_url}/api/generate"

    def get_tags_url(self) -> str:
        return f"{self.base_url}/api/tags"


# ============================================
# 2. CLIENT LLM (L'Adaptateur de Performance - OpenClaw Concept)
# ============================================

class OllamaClient:
    """
    Interface robuste pour interagir avec une instance locale d'Ollama.
    Gère l'authentification et le format des requêtes HTTP.
    """
    def __init__(self, config: AgentConfig):
        self.config = config
        print(f"[INIT] Connecté au client Ollama sur {config.base_url}.")
        if not self.config.default_model:
            self.config.default_model = self._select_installed_model()
        print(f"[INIT] Modèle sélectionné : {self.config.default_model}")

    def _select_installed_model(self) -> str:
        """Sélectionne le modèle demandé par l'environnement ou le premier installé."""
        requested_model = os.environ.get("OLLAMA_MODEL", "").strip()
        request = Request(self.config.get_tags_url(), method="GET")

        try:
            with urlopen(request, timeout=5) as response:
                data = json.loads(response.read().decode("utf-8"))
        except (HTTPError, URLError, TimeoutError, json.JSONDecodeError) as error:
            raise RuntimeError(
                f"Impossible de récupérer les modèles Ollama : {error}"
            ) from error

        models = [item.get("name", "") for item in data.get("models", [])]
        models = [model for model in models if model]
        if not models:
            raise RuntimeError(
                "Aucun modèle Ollama n'est installé. Installez-en un avec 'ollama pull'."
            )
        if requested_model:
            if requested_model not in models:
                raise RuntimeError(
                    f"Le modèle OLLAMA_MODEL='{requested_model}' n'est pas installé. "
                    f"Modèles disponibles : {', '.join(models)}"
                )
            return requested_model
        return models[0]

    def generate_response(self, 
                           prompt: str, 
                           model: str) -> str:
        """
        Envoie une requête générique à Ollama et retourne le texte de réponse.
        
        Args:
            prompt: Le prompt détaillé (Planification, Critique, etc.).
            model: Le nom du modèle à utiliser (ex: "llama3").

        Returns:
            La réponse textuelle du modèle LLM.
        """
        url = self.config.get_ollama_url()
        payload = {
            "model": model,
            "prompt": prompt,
            "stream": False,  # Non-streaming pour la simplicité du prototype
            "format": "json" if 'liste nommée "steps"' in prompt else "",
            "options": {"temperature": 0.1} # Température basse pour la fiabilité/logique
        }

        if not payload["format"]:
            del payload["format"]

        try:
            body = json.dumps(payload).encode("utf-8")
            request = Request(
                url,
                data=body,
                headers={"Content-Type": "application/json; charset=utf-8"},
                method="POST",
            )
            with urlopen(request, timeout=300) as response:
                data = json.loads(response.read().decode("utf-8"))
            return data.get("response", "").strip()

        except HTTPError as error:
            detail = error.read().decode("utf-8", errors="replace")
            raise RuntimeError(
                f"Erreur HTTP Ollama {error.code} pour le modèle '{model}' : {detail}"
            ) from error
        except (URLError, TimeoutError) as error:
            raise RuntimeError(
                f"Impossible de communiquer avec Ollama sur {url} : {error}"
            ) from error
        except json.JSONDecodeError as error:
            raise RuntimeError("Ollama a retourné une réponse JSON invalide.") from error


# ============================================
# 3. MOTEUR D'AGENT (AgentCore - Le Loop Engineering)
# ============================================

class AgentCore:
    """
    Le cœur de l'agent. Implémente le cycle critique de "Loop Engineering" :
    PLAN -> EXECUTE -> REFLECT/CRITIQUE.
    Ce moteur orchestre les appels LLM pour atteindre un objectif complexe.
    """
    def __init__(self, client: OllamaClient):
        self.llm_client = client

    def _simulate_tool_execution(self, action: str) -> str:
        """
        Simule l'exécution d'une action externe (un "outil").
        Dans un vrai système, ceci appellerait des API réelles ou du code Python.
        Est le point de rupture entre la logique et l'environnement réel.
        """
        print(f"\n{'='*20} EXÉCUTION D'ACTION : {action} {'='*20}")
        action_normalized = action.casefold()
        if any(verb in action_normalized for verb in ("rechercher", "analyser", "identifier")):
            return f"Observation: Des résultats de recherche pour '{action}' ont été trouvés. Le point clé est X."
        elif "calculer" in action_normalized:
            # Simule un calcul réussi
            return f"Observation: L'opération mathématique demandée a abouti à la valeur 42."
        else:
            return f"Observation: Action '{action}' exécutée avec succès, mais aucune donnée spécifique n'a été capturée."

    def run_agent(self, user_goal: str) -> str:
        """
        Le cycle principal de l'Agent. 
        Utilise un prompt structuré pour forcer la pensée étape par étape (CoT).
        """
        print("\n" + "="*80)
        print("🚀 DÉBUT DU CYCLE AGENT INTÉGRÉ : PLAN -> EXECUTE -> REFLECT")
        print(f"Objectif utilisateur: {user_goal}")
        print("="*80 + "\n")

        # --- Étape 1: Planification (PLAN) ---
        plan_prompt = f"""
        Tu es un agent d'exécution hautement fiable. L'objectif est le suivant : "{user_goal}".
        Avant de répondre, tu dois nécessairement décomposer cet objectif en une série d'étapes logiques et concrètes actions à effectuer. 
        Ne réponds qu'avec un JSON valide contenant une liste nommée "steps". Chaque étape doit être une chaîne décrivant l'action (ex: 'rechercher le prix de l\'euro') ou la fonction à appeler.
        Exemple de réponse attendue : {{"steps": ["Action 1", "Action 2"]}}
        """
        print("[PHASE 1/3] 🧠 Génération de la planification...")
        plan_response = self.llm_client.generate_response(plan_prompt, self.llm_client.config.default_model)

        try:
            # Tente de charger le JSON généré par l'LLM
            plan_json = json.loads(plan_response)
            steps: List[str] = plan_json.get("steps", [])
            if not isinstance(steps, list) or not all(isinstance(step, str) for step in steps):
                raise TypeError("La propriété 'steps' doit être une liste de chaînes.")
            if not steps:
                raise ValueError("Le modèle a retourné un plan vide.")
            print(f"✅ Planification réussie. Détecté {len(steps)} étapes : {', '.join(steps)}")
        except (json.JSONDecodeError, KeyError, TypeError, ValueError):
            print("\n[ATTENTION] Échec de l'analyse JSON du plan. Traitement des résultats bruts.")
            return f"Erreur critique lors de la planification. Le modèle n'a pas retourné un format JSON valide. Réponse brute reçue: {plan_response}"

        # --- Étape 2: Exécution (EXECUTE) ---
        print("\n" + "="*80)
        print("🏃 PHASE 2/3 : EXÉCUTION DES OUTILS ET ACTIONS")
        observations: List[str] = []

        for i, step in enumerate(steps):
            # Ici on remplace l'appel à un outil réel par une simulation.
            observation = self._simulate_tool_execution(step)
            observations.append(observation)
            print(f"  -> Observation {i+1}: {observation}")

        # --- Étape 3: Réflexion et Finalisation (REFLECT/CRITIQUE) ---
        print("\n" + "="*80)
        print("🧠 PHASE 3/3 : RÉFLEXION ET SYNTHÈSE FINALE")
        
        critique_context = "\n\n--- CONTEXTE D'OBSERVATION ---\n"
        for i, obs in enumerate(observations):
            critique_context += f"Observation {i+1}: {obs}\n"

        reflection_prompt = f"""
        Tu es un agent de synthèse et de critique. L'objectif initial était : "{user_goal}". 
        Nous avons exécuté les actions suivantes, qui ont généré les observations ci-dessous. 
        Ton rôle est double : 
        1. Analyser si toutes les informations nécessaires pour atteindre l'objectif sont présentes. 
        2. Produire la réponse finale et synthétique pour l'utilisateur. 
        N'ajoute aucun autre commentaire, juste une conclusion fluide et professionnelle.

        OBSERVATIONS COMPLÈTES:
        {critique_context}
        """

        final_response = self.llm_client.generate_response(reflection_prompt, self.llm_client.config.default_model)
        return final_response


# ============================================
# 💻 EXÉCUTION PRINCIPALE ET TEST
# ============================================

if __name__ == "__main__":
    configure_utf8_runtime()

    try:
        # Initialisation avec le modèle par défaut (assurez-vous qu'il existe localement)
        config = AgentConfig()
        client = OllamaClient(config=config)
        agent = AgentCore(client=client)

        user_goal_1 = "Détermine la meilleure stratégie d'investissement pour un petit capital. Tu dois commencer par chercher des tendances économiques et calculer le potentiel de croissance."
        
        print("\n##############################################################")
        print("### TEST 1: SCÉNARIO COMPOSÉ (Planification, Tools, Critique) ###")
        final_result = agent.run_agent(user_goal_1)

        print("\n" + "#"*80)
        print("✨ RÉSULTAT FINAL DE L'AGENT ✨".center(80))
        print("-" * 30)
        print(final_result)
        print("#"*80)
    except KeyboardInterrupt:
        print("\n[ARRÊT] Interruption demandée.", file=sys.stderr)
        sys.exit(130)
    except Exception as e:
        print(
            f"\n[ERREUR FATALE] Une exception inattendue est survenue : {e}",
            file=sys.stderr,
        )
        traceback.print_exc()
        sys.exit(1)

